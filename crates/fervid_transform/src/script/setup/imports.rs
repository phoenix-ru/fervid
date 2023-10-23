use swc_core::ecma::{
    ast::{Id, ImportDecl, ImportSpecifier, ModuleExportName},
    atoms::JsWord,
};

use crate::{
    atoms::{VUE, REF, COMPUTED, REACTIVE},
    structs::VueResolvedImports,
};

pub fn collect_imports(
    import_decl: &ImportDecl,
    out: &mut Vec<Id>,
    vue_imports: &mut VueResolvedImports,
) {
    if import_decl.type_only {
        return;
    }

    for specifier in import_decl.specifiers.iter() {
        // examples below are from SWC
        match specifier {
            // e.g. `import * as foo from 'mod.js'`
            ImportSpecifier::Namespace(ns_spec) => out.push(ns_spec.local.to_id()),

            // e.g. `import foo from 'mod.js'`
            ImportSpecifier::Default(default_spec) => out.push(default_spec.local.to_id()),

            // e.g. `import { foo } from 'mod.js'` -> local = foo, imported = None
            // e.g. `import { foo as bar } from 'mod.js'` -> local = bar, imported = Some(foo)
            ImportSpecifier::Named(named_spec) => {
                if named_spec.is_type_only {
                    return;
                }

                if import_decl.src.value == *VUE {
                    if let Some(ref was_renamed_from) = named_spec.imported {
                        // Renamed: `import { ref as r }` or `import { "ref" as r }`
                        let imported_word = match was_renamed_from {
                            ModuleExportName::Ident(ident) => &ident.sym,
                            ModuleExportName::Str(s) => &s.value,
                        };

                        collect_vue_import(imported_word, named_spec.local.to_id(), vue_imports)
                    } else {
                        // Just `import { ref }`
                        collect_vue_import(
                            &named_spec.local.sym,
                            named_spec.local.to_id(),
                            vue_imports,
                        )
                    }
                } else {
                    out.push(named_spec.local.to_id())
                }
            }
        }
    }
}

fn collect_vue_import(imported_word: &JsWord, used_as: Id, vue_imports: &mut VueResolvedImports) {
    if *imported_word == *REF {
        vue_imports.ref_import = Some(used_as)
    } else if *imported_word == *COMPUTED {
        vue_imports.computed = Some(used_as)
    } else if *imported_word == *REACTIVE {
        vue_imports.reactive = Some(used_as)
    }
}

#[cfg(test)]
mod tests {
    use swc_core::{ecma::ast::{Module, ModuleDecl, ModuleItem}, common::SyntaxContext};

    use crate::test_utils::parser::{parse_javascript_module, parse_typescript_module};

    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct MockAnalysisResult {
        imports: Vec<Id>,
        vue_user_imports: VueResolvedImports,
    }

    fn analyze_mock(module: Module) -> MockAnalysisResult {
        let mut imports = Vec::new();
        let mut vue_user_imports = VueResolvedImports::default();

        for module_item in module.body.into_iter() {
            match module_item {
                ModuleItem::ModuleDecl(ModuleDecl::Import(ref import_decl)) => {
                    collect_imports(import_decl, &mut imports, &mut vue_user_imports)
                }

                _ => {}
            }
        }

        MockAnalysisResult {
            imports,
            vue_user_imports,
        }
    }

    fn analyze_js_imports(input: &str) -> MockAnalysisResult {
        let parsed = parse_javascript_module(input, 0, Default::default())
            .expect("analyze_js expects the input to be parseable")
            .0;

        analyze_mock(parsed)
    }

    fn analyze_ts_imports(input: &str) -> MockAnalysisResult {
        let parsed = parse_typescript_module(input, 0, Default::default())
            .expect("analyze_ts expects the input to be parseable")
            .0;

        analyze_mock(parsed)
    }

    macro_rules! test_js_and_ts {
        ($input: expr, $expected: expr) => {
            assert_eq!(analyze_js_imports($input), $expected);
            assert_eq!(analyze_ts_imports($input), $expected);
        };
    }

    #[test]
    fn it_collects_vue_imports() {
        test_js_and_ts!(
            r"
            import { ref, computed, reactive } from 'vue'
            ",
            MockAnalysisResult {
                vue_user_imports: VueResolvedImports {
                    ref_import: Some((JsWord::from("ref"), SyntaxContext::default())),
                    computed: Some((JsWord::from("computed"), SyntaxContext::default())),
                    reactive: Some((JsWord::from("reactive"), SyntaxContext::default()))
                },
                ..Default::default()
            }
        );

        // Aliased
        test_js_and_ts!(
            r"
            import { ref as foo, computed as bar, reactive as baz } from 'vue'
            ",
            MockAnalysisResult {
                vue_user_imports: VueResolvedImports {
                    ref_import: Some((JsWord::from("foo"), SyntaxContext::default())),
                    computed: Some((JsWord::from("bar"), SyntaxContext::default())),
                    reactive: Some((JsWord::from("baz"), SyntaxContext::default()))
                },
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_collects_non_vue_imports() {
        test_js_and_ts!(
            r"
            import { ref } from './vue'
            import { computed } from 'vue-impostor'
            import { reactive } from 'vue/internals'

            import * as foo from './foo'
            import Bar from 'bar-js'
            import { baz, qux } from '@loremipsum/core'
            ",
            MockAnalysisResult {
                imports: vec![
                    (JsWord::from("ref"), SyntaxContext::default()),
                    (JsWord::from("computed"), SyntaxContext::default()),
                    (JsWord::from("reactive"), SyntaxContext::default()),
                    (JsWord::from("foo"), SyntaxContext::default()),
                    (JsWord::from("Bar"), SyntaxContext::default()),
                    (JsWord::from("baz"), SyntaxContext::default()),
                    (JsWord::from("qux"), SyntaxContext::default()),
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_collects_mixed_imports() {
        test_js_and_ts!(
            r"
            import { ref, computed, reactive } from 'vue'

            import * as foo from './foo'
            import Bar from 'bar-js'
            import { baz, qux } from '@loremipsum/core'
            ",
            MockAnalysisResult {
                imports: vec![
                    (JsWord::from("foo"), SyntaxContext::default()),
                    (JsWord::from("Bar"), SyntaxContext::default()),
                    (JsWord::from("baz"), SyntaxContext::default()),
                    (JsWord::from("qux"), SyntaxContext::default()),
                ],
                vue_user_imports: VueResolvedImports {
                    ref_import: Some((JsWord::from("ref"), SyntaxContext::default())),
                    computed: Some((JsWord::from("computed"), SyntaxContext::default())),
                    reactive: Some((JsWord::from("reactive"), SyntaxContext::default()))
                },
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_ignores_type_imports() {
        assert_eq!(
            analyze_ts_imports(
                r"
            import type { ref } from 'vue'
            import type { foo } from './foo'
            import { type computed } from 'vue'
            import { type baz, type qux } from 'baz'
            "
            ),
            MockAnalysisResult::default()
        )
    }
}
