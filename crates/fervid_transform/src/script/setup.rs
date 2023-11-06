use fervid_core::{BindingsHelper, SfcScriptBlock};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        BindingIdent, BlockStmt, Function, Id, Ident, KeyValuePatProp, KeyValueProp, ModuleDecl,
        ModuleItem, ObjectPat, ObjectPatProp, Param, Pat, Prop, PropName, PropOrSpread, Stmt,
    },
};

use crate::{
    atoms::{EMIT, EMITS, EMIT_HELPER, EXPOSE, EXPOSE_HELPER, PROPS, PROPS_HELPER},
    structs::{SfcExportedObjectHelper, VueResolvedImports},
};

mod imports;
mod macros;
mod statements;

pub use imports::*;
pub use statements::*;

use self::macros::postprocess_macros;

pub struct TransformScriptSetupResult {
    /// All the imports (and maybe exports) of the `<script setup>`
    pub module_decls: Vec<ModuleDecl>,
    /// SFC object produced in a form of helper
    pub sfc_object_helper: SfcExportedObjectHelper,
    /// `setup` function produced
    pub setup_fn: Option<Box<Function>>,
}

pub fn transform_and_record_script_setup(
    script_setup: SfcScriptBlock,
    bindings_helper: &mut BindingsHelper,
) -> TransformScriptSetupResult {
    let span = DUMMY_SP; // TODO

    let mut module_decls = Vec::<ModuleDecl>::new();
    let mut sfc_object_helper = SfcExportedObjectHelper::default();

    let mut vue_user_imports = VueResolvedImports::default();
    let mut imports = Vec::<Id>::new();
    let mut setup_body_stmts = Vec::<Stmt>::new();

    // Go over the whole script setup: process all the statements and declarations
    for module_item in script_setup.content.body {
        match module_item {
            ModuleItem::ModuleDecl(decl) => {
                // Collect Vue imports
                // TODO And maybe non-Vue as well?
                if let ModuleDecl::Import(ref import_decl) = decl {
                    collect_imports(import_decl, &mut imports, &mut vue_user_imports);
                }

                module_decls.push(decl);
            }

            ModuleItem::Stmt(stmt) => {
                if let Some(transformed_stmt) = transform_and_record_stmt(
                    stmt,
                    bindings_helper,
                    &vue_user_imports,
                    &mut sfc_object_helper,
                ) {
                    setup_body_stmts.push(transformed_stmt);
                }
            }
        }
    }

    // Post-process macros, e.g. merge models to `props` and `emits`
    postprocess_macros(bindings_helper, &mut sfc_object_helper);

    // Should we check that this function was not assigned anywhere else?
    let setup_fn = Some(Box::new(Function {
        params: get_setup_fn_params(&sfc_object_helper),
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
        setup_fn,
    }
}

pub fn merge_sfc_helper(sfc_helper: SfcExportedObjectHelper, dest: &mut Vec<PropOrSpread>) {
    macro_rules! merge {
        ($field: ident, $span: expr, $sym: expr) => {
            if let Some(value) = sfc_helper.$field {
                dest.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(Ident {
                        span: $span,
                        sym: $sym,
                        optional: false,
                    }),
                    value,
                }))));
            }
        };
    }

    merge!(emits, DUMMY_SP, EMITS.to_owned());
    merge!(props, DUMMY_SP, PROPS.to_owned());

    dest.extend(sfc_helper.untyped_fields);
}

/// Used to populate the params to `setup()`, such as `__props`, `emit`, etc.
fn get_setup_fn_params(sfc_object_helper: &SfcExportedObjectHelper) -> Vec<Param> {
    let has_ctx_param =
        sfc_object_helper.is_setup_emit_referenced || sfc_object_helper.is_setup_expose_referenced;
    let has_props = sfc_object_helper.is_setup_props_referenced || has_ctx_param;

    let result_len = (has_props as usize) + (has_ctx_param as usize);
    let mut result = Vec::<Param>::with_capacity(result_len);

    if has_props {
        result.push(Param {
            span: DUMMY_SP,
            decorators: vec![],
            pat: Pat::Ident(BindingIdent {
                id: Ident {
                    span: DUMMY_SP,
                    sym: PROPS_HELPER.to_owned(),
                    optional: false,
                },
                type_ann: None,
            }),
        });
    }

    if has_ctx_param {
        let mut ctx_props = Vec::<ObjectPatProp>::with_capacity(2);

        macro_rules! add_prop {
            ($prop_sym: expr, $rename_to: expr) => {
                ctx_props.push(ObjectPatProp::KeyValue(KeyValuePatProp {
                    key: swc_core::ecma::ast::PropName::Ident(Ident {
                        span: DUMMY_SP,
                        sym: $prop_sym,
                        optional: false,
                    }),
                    value: Box::new(Pat::Ident(BindingIdent {
                        id: Ident {
                            span: DUMMY_SP,
                            sym: $rename_to,
                            optional: false,
                        },
                        type_ann: None,
                    })),
                }))
            };
        }

        if sfc_object_helper.is_setup_emit_referenced {
            add_prop!(EMIT.to_owned(), EMIT_HELPER.to_owned());
        }
        if sfc_object_helper.is_setup_expose_referenced {
            add_prop!(EXPOSE.to_owned(), EXPOSE_HELPER.to_owned());
        }

        result.push(Param {
            span: DUMMY_SP,
            decorators: vec![],
            pat: Pat::Object(ObjectPat {
                span: DUMMY_SP,
                props: ctx_props,
                optional: false,
                type_ann: None,
            }),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::test_utils::parser::*;
    use fervid_core::{BindingTypes, SfcScriptBlock, SetupBinding, BindingsHelper, FervidAtom};

    use super::transform_and_record_script_setup;

    fn analyze_bindings(script_setup: SfcScriptBlock) -> Vec<SetupBinding> {
        let mut bindings_helper = BindingsHelper::default();
        transform_and_record_script_setup(script_setup, &mut bindings_helper);

        bindings_helper.setup_bindings
    }

    fn analyze_js_bindings(input: &str) -> Vec<SetupBinding> {
        let parsed = parse_javascript_module(input, 0, Default::default())
            .expect("analyze_js expects the input to be parseable")
            .0;

        analyze_bindings(SfcScriptBlock {
            content: Box::new(parsed),
            lang: fervid_core::SfcScriptLang::Es,
            is_setup: true,
        })
    }

    fn analyze_ts_bindings(input: &str) -> Vec<SetupBinding> {
        let parsed = parse_typescript_module(input, 0, Default::default())
            .expect("analyze_ts expects the input to be parseable")
            .0;

        analyze_bindings(SfcScriptBlock {
            content: Box::new(parsed),
            lang: fervid_core::SfcScriptLang::Typescript,
            is_setup: true,
        })
    }

    macro_rules! test_js_and_ts {
        ($input: expr, $expected: expr) => {
            assert_eq!(analyze_js_bindings($input), $expected);
            assert_eq!(analyze_ts_bindings($input), $expected);
        };
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
            vec![
                SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupRef),
                SetupBinding(FervidAtom::from("bar"), BindingTypes::SetupRef),
                SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupRef),
                SetupBinding(FervidAtom::from("qux"), BindingTypes::SetupRef),
            ] // vue_imports: VueResolvedImports {
              //     ref_import: Some((FervidAtom::from("ref"), SyntaxContext::default())),
              //     computed: Some((FervidAtom::from("computed"), SyntaxContext::default())),
              //     reactive: None
              // },
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
            // imports: vec![
            //     (FervidAtom::from("ref"), SyntaxContext::default()),
            //     (FervidAtom::from("computed"), SyntaxContext::default()),
            //     (FervidAtom::from("reactive"), SyntaxContext::default()),
            // ],
            vec![
                SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("bar"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("qux"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("rea"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("reb"), BindingTypes::SetupMaybeRef),
            ]
        );
    }

    #[test]
    fn it_supports_ts_enums() {
        assert_eq!(
            analyze_ts_bindings(
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
            vec![
                SetupBinding(FervidAtom::from("Foo"), BindingTypes::LiteralConst),
                SetupBinding(FervidAtom::from("Bar"), BindingTypes::LiteralConst),
                SetupBinding(FervidAtom::from("Baz"), BindingTypes::LiteralConst),
                SetupBinding(FervidAtom::from("Qux"), BindingTypes::LiteralConst),
            ]
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
            // vue_imports: VueResolvedImports {
            //     ref_import: Some((FervidAtom::from("ref"), SyntaxContext::default())),
            //     computed: Some((FervidAtom::from("computed"), SyntaxContext::default())),
            //     reactive: Some((FervidAtom::from("reactive"), SyntaxContext::default()))
            // },
            vec![
                SetupBinding(FervidAtom::from("cstFoo"), BindingTypes::SetupRef),
                SetupBinding(FervidAtom::from("cstBar"), BindingTypes::SetupRef),
                SetupBinding(FervidAtom::from("cstBaz"), BindingTypes::SetupReactiveConst),
                SetupBinding(FervidAtom::from("letFoo"), BindingTypes::SetupLet),
                SetupBinding(FervidAtom::from("letBar"), BindingTypes::SetupLet),
                SetupBinding(FervidAtom::from("letBaz"), BindingTypes::SetupLet),
                SetupBinding(FervidAtom::from("varFoo"), BindingTypes::SetupLet),
                SetupBinding(FervidAtom::from("varBar"), BindingTypes::SetupLet),
                SetupBinding(FervidAtom::from("varBaz"), BindingTypes::SetupLet),
            ]
        );
    }
}
