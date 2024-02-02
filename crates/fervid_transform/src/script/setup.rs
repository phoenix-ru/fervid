use fervid_core::{BindingTypes, BindingsHelper, SetupBinding, SfcScriptBlock};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        BindingIdent, BlockStmt, Decl, ExprStmt, Function, Ident, KeyValuePatProp, KeyValueProp,
        ModuleDecl, ModuleItem, ObjectPat, ObjectPatProp, Param, Pat, Prop, PropName, PropOrSpread,
        Stmt, VarDeclKind,
    },
};

use crate::{
    atoms::{EMIT, EMITS, EMIT_HELPER, EXPOSE, EXPOSE_HELPER, PROPS, PROPS_HELPER},
    script::{
        common::{
            categorize_class, categorize_expr, categorize_fn_decl, enrich_binding_types,
            extract_variables_from_pat,
        },
        setup::macros::TransformMacroResult,
    },
    structs::{SfcExportedObjectHelper, VueResolvedImports},
};

mod imports;
mod macros;

pub use imports::*;

use self::macros::{postprocess_macros, transform_script_setup_macro_expr};

pub struct TransformScriptSetupResult {
    /// All the imports (and maybe exports) of the `<script setup>`
    pub module_decls: Vec<ModuleDecl>,
    /// SFC object produced in a form of helper
    pub sfc_object_helper: SfcExportedObjectHelper,
    /// `setup` function produced
    pub setup_fn: Option<Box<Function>>,
}

/// Transforms the `<script setup>` block and records its bindings
pub fn transform_and_record_script_setup(
    script_setup: SfcScriptBlock,
    bindings_helper: &mut BindingsHelper,
) -> TransformScriptSetupResult {
    let span = script_setup.content.span;

    let mut module_decls = Vec::<ModuleDecl>::new();
    let mut sfc_object_helper = SfcExportedObjectHelper::default();

    let mut vue_user_imports = VueResolvedImports::default();
    let mut setup_body_stmts = Vec::<Stmt>::new();

    // Collect imports first
    // This is because ES6 imports are hoisted and usage like this is valid:
    // const bar = x(1)
    // import { reactive as x } from 'vue'
    for module_item in script_setup.content.body.iter() {
        if let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = module_item {
            collect_imports(
                import_decl,
                &mut bindings_helper.setup_bindings,
                &mut vue_user_imports,
            );
        };
    }

    // Go over the whole script setup: process all the statements and declarations
    for module_item in script_setup.content.body {
        let stmt = match module_item {
            ModuleItem::ModuleDecl(decl) => {
                module_decls.push(decl);
                continue;
            },
            ModuleItem::Stmt(stmt) => stmt,
        };

        let transformed = match stmt {
            Stmt::Expr(expr_stmt) => {
                let span = expr_stmt.span;

                let transform_macro_result = transform_script_setup_macro_expr(
                    &expr_stmt.expr,
                    bindings_helper,
                    &mut sfc_object_helper,
                    false,
                );

                match transform_macro_result {
                    TransformMacroResult::ValidMacro(transformed_expr) => {
                        // A macro may overwrite the statement
                        transformed_expr.map(|expr| Stmt::Expr(ExprStmt { span, expr }))
                    }

                    TransformMacroResult::NotAMacro => {
                        // No analysis necessary, return the same statement
                        Some(Stmt::Expr(expr_stmt))
                    }
                }
            }

            Stmt::Decl(decl) => transform_decl_stmt(
                decl,
                bindings_helper,
                &vue_user_imports,
                &mut sfc_object_helper,
            )
            .map(Stmt::Decl),

            // By default, just return the same statement
            _ => Some(stmt),
        };

        if let Some(transformed_stmt) = transformed {
            setup_body_stmts.push(transformed_stmt);
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
        is_async: sfc_object_helper.is_async_setup,
        type_params: None,
        return_type: None,
    }));

    TransformScriptSetupResult {
        module_decls,
        sfc_object_helper,
        setup_fn,
    }
}

/// Analyzes the declaration in `script setup` context.
/// These are typically `var`/`let`/`const` declarations, function declarations, etc.
fn transform_decl_stmt(
    decl: Decl,
    bindings_helper: &mut BindingsHelper,
    vue_user_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Decl> {
    /// Pushes the binding type and returns the same passed `Decl`
    macro_rules! push_return {
        ($binding: expr) => {
            bindings_helper.setup_bindings.push($binding);
            // By default, just return the same declaration
            return Some(decl);
        };
    }

    match decl {
        Decl::Class(ref class) => {
            push_return!(categorize_class(class));
        }

        Decl::Fn(ref fn_decl) => {
            push_return!(categorize_fn_decl(fn_decl));
        }

        Decl::Var(mut var_decl) => {
            let is_const = matches!(var_decl.kind, VarDeclKind::Const);

            // Collected bindings cache
            let mut collected_bindings = Vec::<SetupBinding>::with_capacity(2);

            for var_declarator in var_decl.as_mut().decls.iter_mut() {
                // LHS is just an identifier, e.g. in `const foo = 'bar'`
                let is_ident = var_declarator.name.is_ident();

                // Extract all the variables from the LHS (these are mostly suggestions)
                extract_variables_from_pat(&var_declarator.name, &mut collected_bindings, is_const);

                // Process RHS
                if let Some(ref init_expr) = var_declarator.init {
                    let transform_macro_result = transform_script_setup_macro_expr(
                        init_expr,
                        bindings_helper,
                        sfc_object_helper,
                        true,
                    );

                    if let TransformMacroResult::ValidMacro(transformed_expr) =
                        transform_macro_result
                    {
                        // Macros always overwrite the RHS
                        var_declarator.init = transformed_expr;
                    } else if is_const && is_ident {
                        // Resolve only when this is a constant identifier.
                        // For destructures correct bindings are already assigned.
                        let rhs_type = categorize_expr(
                            init_expr,
                            vue_user_imports,
                            &mut sfc_object_helper.is_async_setup,
                        );

                        enrich_binding_types(&mut collected_bindings, rhs_type, is_const, is_ident);
                    }
                }

                bindings_helper
                    .setup_bindings
                    .extend(collected_bindings.drain(..));
            }

            Some(Decl::Var(var_decl))
        }

        Decl::TsEnum(ref ts_enum) => {
            // Ambient enums are also included, this is intentional
            // I am not sure about `const enum`s though
            push_return!(SetupBinding(
                ts_enum.id.sym.to_owned(),
                BindingTypes::LiteralConst,
            ));
        }

        // TODO: What?
        // Decl::TsInterface(_) => todo!(),
        // Decl::TsTypeAlias(_) => todo!(),
        // Decl::TsModule(_) => todo!(),
        _ => Some(decl),
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
    use fervid_core::{fervid_atom, BindingTypes, BindingsHelper, SetupBinding, SfcScriptBlock};
    use swc_core::common::DUMMY_SP;

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
            span: DUMMY_SP,
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
            span: DUMMY_SP,
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
                SetupBinding(fervid_atom!("foo"), BindingTypes::SetupRef),
                SetupBinding(fervid_atom!("bar"), BindingTypes::SetupRef),
                SetupBinding(fervid_atom!("baz"), BindingTypes::SetupRef),
                SetupBinding(fervid_atom!("qux"), BindingTypes::SetupRef),
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
            vec![
                SetupBinding(fervid_atom!("ref"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("computed"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("reactive"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("foo"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("bar"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("baz"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("qux"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("rea"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("reb"), BindingTypes::SetupMaybeRef),
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
                SetupBinding(fervid_atom!("Foo"), BindingTypes::LiteralConst),
                SetupBinding(fervid_atom!("Bar"), BindingTypes::LiteralConst),
                SetupBinding(fervid_atom!("Baz"), BindingTypes::LiteralConst),
                SetupBinding(fervid_atom!("Qux"), BindingTypes::LiteralConst),
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
                SetupBinding(fervid_atom!("cstFoo"), BindingTypes::SetupRef),
                SetupBinding(fervid_atom!("cstBar"), BindingTypes::SetupRef),
                SetupBinding(fervid_atom!("cstBaz"), BindingTypes::SetupReactiveConst),
                SetupBinding(fervid_atom!("letFoo"), BindingTypes::SetupLet),
                SetupBinding(fervid_atom!("letBar"), BindingTypes::SetupLet),
                SetupBinding(fervid_atom!("letBaz"), BindingTypes::SetupLet),
                SetupBinding(fervid_atom!("varFoo"), BindingTypes::SetupLet),
                SetupBinding(fervid_atom!("varBar"), BindingTypes::SetupLet),
                SetupBinding(fervid_atom!("varBaz"), BindingTypes::SetupLet),
            ]
        );
    }

    // Cases from official spec
    // https://github.com/vuejs/core/blob/a41c5f1f4367a9f41bcdb8c4e02f54b2378e577d/packages/compiler-sfc/__tests__/compileScript.spec.ts

    #[test]
    fn import_ref_reactive_function_from_other_place_directly() {
        test_js_and_ts!(
            r"
            import { ref, reactive } from './foo'

            const foo = ref(1)
            const bar = reactive(1)
            ",
            vec![
                SetupBinding(fervid_atom!("ref"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("reactive"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("foo"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("bar"), BindingTypes::SetupMaybeRef),
            ]
        );
    }

    #[test]
    fn import_ref_reactive_function_from_other_place_import_w_alias() {
        test_js_and_ts!(
            r"
            import { ref as _ref, reactive as _reactive } from './foo'

            const foo = ref(1)
            const bar = reactive(1)
            ",
            vec![
                SetupBinding(fervid_atom!("_ref"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("_reactive"), BindingTypes::Imported),
                SetupBinding(fervid_atom!("foo"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("bar"), BindingTypes::SetupMaybeRef),
            ]
        );
    }

    #[test]
    fn import_ref_reactive_function_from_other_place_aliased_usage_before_import_site() {
        test_js_and_ts!(
            r"
            const bar = x(1)
            import { reactive as x } from 'vue'
            ",
            vec![SetupBinding(
                fervid_atom!("bar"),
                BindingTypes::SetupReactiveConst
            ),]
        );
    }

    #[test]
    fn should_support_module_string_names_syntax() {
        // TODO Dedupe and two scripts
        // https://github.com/vuejs/core/blob/a41c5f1f4367a9f41bcdb8c4e02f54b2378e577d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L326
        test_js_and_ts!(
            r#"
            import { "😏" as foo } from './foo'
            "#,
            vec![SetupBinding(fervid_atom!("foo"), BindingTypes::Imported),]
        );
    }

    #[test]
    fn with_typescript_hoist_type_declarations() {
        assert_eq!(
            analyze_ts_bindings(
                r"
                export interface Foo {}
                type Bar = {}
                "
            ),
            vec![]
        );
    }

    #[test]
    fn with_typescript_runtime_enum() {
        assert_eq!(
            analyze_ts_bindings(
                r"
                enum Foo { A = 123 }
                "
            ),
            vec![SetupBinding(
                fervid_atom!("Foo"),
                BindingTypes::LiteralConst
            )]
        );
    }

    #[test]
    fn with_typescript_runtime_enum_in_normal_script() {
        // TODO Two scripts
        // https://github.com/vuejs/core/blob/a41c5f1f4367a9f41bcdb8c4e02f54b2378e577d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L898
    }

    #[test]
    fn with_typescript_const_enum() {
        assert_eq!(
            analyze_ts_bindings(
                r"
                const enum Foo { A = 123 }
                "
            ),
            vec![SetupBinding(
                fervid_atom!("Foo"),
                BindingTypes::LiteralConst
            )]
        );
    }

    #[test]
    fn with_typescript_import_type() {
        assert_eq!(
            analyze_ts_bindings(
                r"
                import type { Foo } from './main.ts'
                import { type Bar, Baz } from './main.ts'
                "
            ),
            vec![SetupBinding(
                fervid_atom!("Baz"),
                BindingTypes::Imported
            )]
        );
    }

    #[test]
    fn with_typescript_with_generic_attribute() {
        // TODO Generics are not implemented yet
        // https://github.com/vuejs/core/blob/a41c5f1f4367a9f41bcdb8c4e02f54b2378e577d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L942
    }
}
