use fervid_core::{BindingTypes, SetupBinding};
use swc_core::ecma::{
    ast::{Id, Ident, ImportDecl, ImportSpecifier, ModuleExportName},
    atoms::JsWord,
};

use crate::{
    atoms::{COMPUTED, REACTIVE, REF, VUE},
    structs::VueResolvedImports,
};

pub fn collect_imports(
    import_decl: &ImportDecl,
    out: &mut Vec<SetupBinding>,
    vue_imports: &mut VueResolvedImports,
) {
    if import_decl.type_only {
        return;
    }

    // Checks if the import is of `.vue`, i.e. eligible to be a component
    // Should this handle complex queries?
    let is_dot_vue_import = import_decl.src.value.ends_with(".vue");

    // Check if this is a `from 'vue'`
    let is_from_vue_import = import_decl.src.value == *VUE;

    for specifier in import_decl.specifiers.iter() {
        // examples below are from SWC
        match specifier {
            // e.g. `import * as foo from 'mod.js'`
            // not a default export, thus never suitable to be a `Component`
            ImportSpecifier::Namespace(ns_spec) => out.push(SetupBinding(
                ns_spec.local.sym.to_owned(),
                BindingTypes::Imported,
            )),

            // e.g. `import foo from 'mod.js'`
            ImportSpecifier::Default(default_spec) => {
                let binding_type = if is_dot_vue_import {
                    BindingTypes::Component
                } else {
                    BindingTypes::Imported
                };
                out.push(SetupBinding(
                    default_spec.local.sym.to_owned(),
                    binding_type,
                ));
            }

            // e.g. `import { foo } from 'mod.js'` -> local = foo, imported = None
            // e.g. `import { foo as bar } from 'mod.js'` -> local = bar, imported = Some(foo)
            ImportSpecifier::Named(named_spec) => {
                if named_spec.is_type_only {
                    return;
                }

                // `imported_as` is the variable name, `imported_word` is the imported symbol
                // `import { foo as bar } from 'baz'` -> `imported_as` is `bar`, `imported_word` is `foo`
                let imported_as: &Ident = &named_spec.local;
                let imported_word = match named_spec.imported.as_ref() {
                    // Renamed, e.g. `import { ref as r }` or `import { "ref" as r }`
                    Some(ModuleExportName::Ident(ident)) => &ident.sym,
                    Some(ModuleExportName::Str(s)) => &s.value,
                    // Not renamed, e.g. `import { ref }`
                    None => &imported_as.sym,
                };

                if is_from_vue_import {
                    collect_vue_import(imported_word, imported_as.to_id(), vue_imports);
                } else if is_dot_vue_import && imported_word == "default" {
                    // Only `import { default as Smth }` is supported.
                    // `import { default }` is invalid, and SWC will catch that
                    out.push(SetupBinding(
                        imported_as.sym.to_owned(),
                        BindingTypes::Component,
                    ));
                } else {
                    out.push(SetupBinding(
                        imported_as.sym.to_owned(),
                        BindingTypes::Imported,
                    ));
                }
            }
        }
    }
}

#[inline]
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
    use fervid_core::fervid_atom;
    use swc_core::{
        common::SyntaxContext,
        ecma::ast::{Module, ModuleDecl, ModuleItem},
    };

    use crate::test_utils::parser::{parse_javascript_module, parse_typescript_module};

    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct MockAnalysisResult {
        imports: Vec<SetupBinding>,
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
                    ref_import: Some((fervid_atom!("ref"), SyntaxContext::default())),
                    computed: Some((fervid_atom!("computed"), SyntaxContext::default())),
                    reactive: Some((fervid_atom!("reactive"), SyntaxContext::default()))
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
                    ref_import: Some((fervid_atom!("foo"), SyntaxContext::default())),
                    computed: Some((fervid_atom!("bar"), SyntaxContext::default())),
                    reactive: Some((fervid_atom!("baz"), SyntaxContext::default()))
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
                    SetupBinding(fervid_atom!("ref"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("computed"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("reactive"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("foo"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("Bar"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("baz"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("qux"), BindingTypes::Imported),
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
                    SetupBinding(fervid_atom!("foo"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("Bar"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("baz"), BindingTypes::Imported),
                    SetupBinding(fervid_atom!("qux"), BindingTypes::Imported),
                ],
                vue_user_imports: VueResolvedImports {
                    ref_import: Some((fervid_atom!("ref"), SyntaxContext::default())),
                    computed: Some((fervid_atom!("computed"), SyntaxContext::default())),
                    reactive: Some((fervid_atom!("reactive"), SyntaxContext::default()))
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
