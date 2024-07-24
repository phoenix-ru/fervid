use fervid_core::{fervid_atom, BindingTypes, FervidAtom};
use swc_core::ecma::{
    ast::{Id, Ident, ImportSpecifier, Module, ModuleDecl, ModuleExportName, ModuleItem},
    atoms::JsWord,
};

use crate::{
    atoms::{COMPUTED, DEFINE_EMITS, DEFINE_EXPOSE, DEFINE_PROPS, REACTIVE, REF, VUE},
    error::{ScriptError, ScriptErrorKind, TransformError },
    structs::VueResolvedImports,
    BindingsHelper, ImportBinding, SetupBinding,
};

/// Collects imports and removes duplicates
pub fn process_imports(
    module: &mut Module,
    bindings_helper: &mut BindingsHelper,
    is_from_setup: bool,
    errors: &mut Vec<TransformError>,
) {
    module.body.retain_mut(|module_item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = module_item else {
            return true;
        };

        // Do not process type-only declarations, do not collect
        if import_decl.type_only {
            return true;
        }

        let source = &import_decl.src.value;

        // Checks if the import is of `.vue`, i.e. eligible to be a component
        // Should this handle complex queries?
        let is_dot_vue_import = source.ends_with(".vue");

        // Check if this is a `from 'vue'`
        let is_from_vue_import = *source == *VUE;

        let prev_len = import_decl.specifiers.len();

        import_decl.specifiers.retain(|specifier| {
            register_import(
                specifier,
                bindings_helper,
                source,
                is_from_setup,
                is_dot_vue_import,
                is_from_vue_import,
                errors,
            )
        });

        // Do not retain emptied imports, i.e. the fully deduplicated ones (`import { foo } from './foo'` -> `import {} from './foo'`).
        // This is not a side effect, because we only removed duplicate imports
        !(prev_len > 0 && import_decl.specifiers.is_empty())
    });
}

/// Returns whether an import should be preserved
pub fn register_import(
    import_specifier: &ImportSpecifier,
    bindings_helper: &mut BindingsHelper,
    source: &FervidAtom,
    is_from_setup: bool,
    is_dot_vue_import: bool,
    is_from_vue_import: bool,
    errors: &mut Vec<TransformError>,
) -> bool {
    let mut binding_type = BindingTypes::Imported;

    let (local, imported, span) = match import_specifier {
        // e.g. `import * as foo from 'mod.js'`
        // not a default export, thus never suitable to be a `Component`
        ImportSpecifier::Namespace(ns_spec) => (
            ns_spec.local.sym.to_owned(),
            fervid_atom!("*"),
            ns_spec.span,
        ),

        // e.g. `import foo from 'mod.js'`
        ImportSpecifier::Default(default_spec) => {
            if is_dot_vue_import {
                binding_type = BindingTypes::Component
            }
            (
                default_spec.local.sym.to_owned(),
                fervid_atom!("default"),
                default_spec.span,
            )
        }

        // e.g. `import { foo } from 'mod.js'` -> local = foo, imported = None
        // e.g. `import { foo as bar } from 'mod.js'` -> local = bar, imported = Some(foo)
        ImportSpecifier::Named(named_spec) => {
            if named_spec.is_type_only {
                return true;
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
                // Warn about compiler macros
                if *imported_word == *DEFINE_PROPS
                    || *imported_word == *DEFINE_EMITS
                    || *imported_word == *DEFINE_EXPOSE
                {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: named_spec.span,
                        kind: ScriptErrorKind::CompilerMacroImport,
                    }));
                    return false;
                }

                collect_vue_import(
                    imported_word,
                    imported_as.to_id(),
                    &mut bindings_helper.vue_resolved_imports,
                );
                return true;
            } else if is_dot_vue_import && imported_word == "default" {
                // Only `import { default as Smth }` is supported.
                // `import { default }` is invalid, and SWC will catch that
                binding_type = BindingTypes::Component;
            }

            (
                imported_as.sym.to_owned(),
                imported_word.to_owned(),
                named_spec.span,
            )
        }
    };

    // Check duplicates
    if let Some(existing) = bindings_helper.user_imports.get(&local) {
        // Not exact duplicate means some local name has been used twice
        if existing.source != *source || existing.imported != imported {
            errors.push(TransformError::ScriptError(ScriptError {
                span,
                kind: ScriptErrorKind::DuplicateImport,
            }));
        }

        return false;
    }

    if is_from_setup {
        bindings_helper
            .setup_bindings
            .push(SetupBinding(local.to_owned(), BindingTypes::Imported))
    } else {
        let bindings = bindings_helper
            .options_api_bindings
            .get_or_insert_with(|| Default::default());
        bindings
            .imports
            .push(SetupBinding(local.to_owned(), binding_type));
    }

    bindings_helper.user_imports.insert(
        local.to_owned(),
        ImportBinding {
            source: source.to_owned(),
            imported,
            local,
            is_from_setup,
        },
    );

    true
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
    use swc_core::{common::SyntaxContext, ecma::ast::Module};

    use crate::test_utils::parser::{parse_javascript_module, parse_typescript_module};

    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct MockAnalysisResult {
        imports: Vec<SetupBinding>,
        vue_user_imports: VueResolvedImports,
    }

    fn analyze_mock(mut module: Module) -> MockAnalysisResult {
        let mut bindings_helper = Default::default();
        let mut errors = Vec::new();

        process_imports(
            &mut module,
            &mut bindings_helper,
            true,
            &mut errors,
        );

        MockAnalysisResult {
            imports: bindings_helper.setup_bindings,
            vue_user_imports: *bindings_helper.vue_resolved_imports,
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

    #[test]
    fn it_deduplicates_imports() {
        
    }
}
