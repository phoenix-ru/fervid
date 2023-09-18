use fervid_core::SfcScriptBlock;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{BlockStmt, Function, Id, ModuleDecl, ModuleItem, Stmt},
};

use crate::structs::{ScopeHelper, VueResolvedImports, SfcExportedObjectHelper};

mod imports;
mod macros;
mod statements;

pub use imports::*;
pub use statements::*;

pub struct TransformScriptSetupResult {
    /// All the imports (and maybe exports) of the `<script setup>`
    pub module_decls: Vec<ModuleDecl>,
    /// SFC object produced in a form of helper
    pub sfc_object_helper: SfcExportedObjectHelper,
    /// `setup` function produced
    pub setup_fn: Option<Box<Function>>
}

pub fn transform_and_record_script_setup(
    script_setup: SfcScriptBlock,
    scope_helper: &mut ScopeHelper,
) -> TransformScriptSetupResult {
    let span = DUMMY_SP; // TODO

    let mut module_decls = Vec::<ModuleDecl>::new();
    let mut sfc_object_helper = SfcExportedObjectHelper::default();

    let mut vue_imports = VueResolvedImports::default();
    let mut imports = Vec::<Id>::new();
    let mut setup_body_stmts = Vec::<Stmt>::new();

    for module_item in script_setup.content.body {
        match module_item {
            ModuleItem::ModuleDecl(decl) => {
                // Collect Vue imports
                // TODO And maybe non-Vue as well?
                if let ModuleDecl::Import(ref import_decl) = decl {
                    collect_imports(import_decl, &mut imports, &mut vue_imports);
                }

                module_decls.push(decl);
            }

            ModuleItem::Stmt(stmt) => {
                // todo actual analysis and transformation as in `fervid_script`
                if let Some(transformed_stmt) = transform_and_record_stmt(
                    &stmt,
                    &mut scope_helper.setup_bindings,
                    &vue_imports,
                    &mut sfc_object_helper,
                ) {
                    setup_body_stmts.push(transformed_stmt);
                }
            }
        }
    }

    // Should we check that this function was not assigned anywhere else?
    let setup_fn = Some(Box::new(Function {
        params: vec![],
        decorators: vec![],
        span,
        body: Some(BlockStmt {
            span,
            stmts: setup_body_stmts,
        }),
        is_generator: false,
        is_async: false, // TODO
        type_params: None,
        return_type: None,
    }));

    TransformScriptSetupResult {
        module_decls,
        sfc_object_helper,
        setup_fn
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        script::setup::{collect_imports, transform_and_record_stmt},
        structs::{SetupBinding, VueResolvedImports},
        test_utils::parser::*,
    };
    use fervid_core::BindingTypes;
    use swc_core::{
        common::SyntaxContext,
        ecma::{
            ast::{Id, Module, ModuleDecl, ModuleItem},
            atoms::JsWord,
        },
    };

    #[derive(Debug, Default, PartialEq)]
    struct MockAnalysisResult {
        imports: Vec<Id>,
        vue_imports: VueResolvedImports,
        setup: Vec<SetupBinding>,
    }

    fn analyze_mock(module: &Module) -> MockAnalysisResult {
        let mut imports = Vec::new();
        let mut vue_imports = VueResolvedImports::default();
        let mut setup = Vec::new();
        let mut sfc_object = Default::default();

        for module_item in module.body.iter() {
            match *module_item {
                ModuleItem::ModuleDecl(ModuleDecl::Import(ref import_decl)) => {
                    collect_imports(import_decl, &mut imports, &mut vue_imports)
                }

                ModuleItem::Stmt(ref stmt) => {
                    transform_and_record_stmt(stmt, &mut setup, &mut vue_imports, &mut sfc_object);
                }

                // Exports are ignored (ModuleDecl::Export* and ModuleDecl::Ts*)
                _ => {}
            }
        }

        MockAnalysisResult {
            imports,
            vue_imports,
            setup,
        }
    }

    fn analyze_js(input: &str) -> MockAnalysisResult {
        let parsed = parse_javascript_module(input, 0, Default::default())
            .expect("analyze_js expects the input to be parseable")
            .0;

        analyze_mock(&parsed)
    }

    fn analyze_ts(input: &str) -> MockAnalysisResult {
        let parsed = parse_typescript_module(input, 0, Default::default())
            .expect("analyze_ts expects the input to be parseable")
            .0;

        analyze_mock(&parsed)
    }

    macro_rules! test_js_and_ts {
        ($input: expr, $expected: expr) => {
            assert_eq!(analyze_js($input), $expected);
            assert_eq!(analyze_ts($input), $expected);
        };
    }

    #[test]
    fn it_collects_vue_imports() {
        test_js_and_ts!(
            r"
            import { ref, computed, reactive } from 'vue'
            ",
            MockAnalysisResult {
                vue_imports: VueResolvedImports {
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
                vue_imports: VueResolvedImports {
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
                vue_imports: VueResolvedImports {
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
            analyze_ts(
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
    fn it_collects_refs() {
        test_js_and_ts!(
            r"
            import { ref, computed } from 'vue'

            const foo = ref()
            const bar = ref(42)
            const baz = computed()
            const qux = computed(() => 42)
            ",
            MockAnalysisResult {
                setup: vec![
                    SetupBinding(JsWord::from("foo"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("bar"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("baz"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("qux"), BindingTypes::SetupRef),
                ],
                vue_imports: VueResolvedImports {
                    ref_import: Some((JsWord::from("ref"), SyntaxContext::default())),
                    computed: Some((JsWord::from("computed"), SyntaxContext::default())),
                    reactive: None
                },
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_recognizes_non_vue_refs() {
        test_js_and_ts!(
            r"
            import { ref } from './vue'
            import { computed } from 'vue-impostor'
            import { reactive } from 'vue/internals'

            const foo = ref()
            const bar = ref(42)
            const baz = computed()
            const qux = computed(() => 42)
            const rea = reactive()
            const reb = reactive({})
            ",
            MockAnalysisResult {
                imports: vec![
                    (JsWord::from("ref"), SyntaxContext::default()),
                    (JsWord::from("computed"), SyntaxContext::default()),
                    (JsWord::from("reactive"), SyntaxContext::default()),
                ],
                setup: vec![
                    SetupBinding(JsWord::from("foo"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("bar"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("baz"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("qux"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("rea"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("reb"), BindingTypes::SetupMaybeRef),
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_supports_ts_enums() {
        assert_eq!(
            analyze_ts(
                r"
            enum Foo {}
            const enum Bar {
                One,
                Two,
                Three
            }

            // Ambient enums are also supported
            // Compiler will assume they are available to the module
            declare enum Baz {}
            declare const enum Qux {
                AmbientOne,
                AmbientTwo
            }
            "
            ),
            MockAnalysisResult {
                setup: vec![
                    SetupBinding(JsWord::from("Foo"), BindingTypes::LiteralConst),
                    SetupBinding(JsWord::from("Bar"), BindingTypes::LiteralConst),
                    SetupBinding(JsWord::from("Baz"), BindingTypes::LiteralConst),
                    SetupBinding(JsWord::from("Qux"), BindingTypes::LiteralConst),
                ],
                ..Default::default()
            }
        )
    }

    #[test]
    fn it_supports_multi_declarations() {
        test_js_and_ts!(
            r"
            import { ref, computed, reactive } from 'vue'

            const
                cstFoo = ref('foo'),
                cstBar = computed(() => 42),
                cstBaz = reactive({ qux: true })

            let
                letFoo = ref('foo'),
                letBar = computed(() => 42),
                letBaz = reactive({ qux: true })

            var
                varFoo = ref('foo'),
                varBar = computed(() => 42),
                varBaz = reactive({ qux: true })
            ",
            MockAnalysisResult {
                vue_imports: VueResolvedImports {
                    ref_import: Some((JsWord::from("ref"), SyntaxContext::default())),
                    computed: Some((JsWord::from("computed"), SyntaxContext::default())),
                    reactive: Some((JsWord::from("reactive"), SyntaxContext::default()))
                },
                setup: vec![
                    SetupBinding(JsWord::from("cstFoo"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("cstBar"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("cstBaz"), BindingTypes::SetupReactiveConst),
                    SetupBinding(JsWord::from("letFoo"), BindingTypes::SetupLet),
                    SetupBinding(JsWord::from("letBar"), BindingTypes::SetupLet),
                    SetupBinding(JsWord::from("letBaz"), BindingTypes::SetupLet),
                    SetupBinding(JsWord::from("varFoo"), BindingTypes::SetupLet),
                    SetupBinding(JsWord::from("varBar"), BindingTypes::SetupLet),
                    SetupBinding(JsWord::from("varBaz"), BindingTypes::SetupLet),
                ],
                ..Default::default()
            }
        );
    }
}
