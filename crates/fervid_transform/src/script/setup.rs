use fervid_core::{BindingTypes, SfcScriptBlock};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::ast::{
        BindingIdent, BlockStmt, Decl, ExprStmt, Function, Ident, KeyValuePatProp, KeyValueProp,
        ModuleDecl, ModuleItem, ObjectPat, ObjectPatProp, Param, Pat, Prop, PropName, PropOrSpread,
        Stmt, VarDeclKind,
    },
};

use crate::{
    atoms::{EMIT, EMITS, EMIT_HELPER, EXPOSE, EXPOSE_HELPER, PROPS, PROPS_HELPER},
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::{
        common::{
            categorize_class, categorize_expr, categorize_fn_decl, enrich_binding_types,
            extract_variables_from_pat,
        },
        setup::macros::TransformMacroResult,
        utils::is_static,
    },
    structs::SfcExportedObjectHelper,
    SetupBinding, TransformSfcContext,
};

mod await_detection;
mod define_props;
mod macros;

use self::{
    await_detection::detect_await_module_item,
    macros::{postprocess_macros, transform_script_setup_macro_expr},
};

use super::resolve_type::TypeResolveContext;

pub struct TransformScriptSetupResult {
    /// All the imports (and maybe exports) of the `<script setup>`
    pub module_items: Vec<ModuleItem>,
    /// SFC object produced in a form of helper
    pub sfc_object_helper: SfcExportedObjectHelper,
    /// `setup` function produced
    pub setup_fn: Option<Box<Function>>,
}

/// Transforms the `<script setup>` block and records its bindings
pub fn transform_and_record_script_setup(
    ctx: &mut TransformSfcContext,
    script_setup: SfcScriptBlock,
    errors: &mut Vec<TransformError>,
) -> TransformScriptSetupResult {
    let span = script_setup.content.span;

    let mut module_items = Vec::<ModuleItem>::new();
    let mut sfc_object_helper = SfcExportedObjectHelper::default();

    let mut setup_body_stmts = Vec::<Stmt>::new();

    // Detect `await` usage
    for module_item in script_setup.content.body.iter() {
        if sfc_object_helper.is_async_setup {
            break;
        }

        sfc_object_helper.is_async_setup |= detect_await_module_item(module_item);
    }

    // Go over the whole script setup: process all the statements and declarations
    for module_item in script_setup.content.body {
        let stmt = match module_item {
            ModuleItem::ModuleDecl(ref decl) => {
                // Disallow non-type exports
                let setup_export_error_span: Option<Span> = check_export(decl);
                match setup_export_error_span {
                    Some(span) => errors.push(TransformError::ScriptError(ScriptError {
                        span,
                        kind: ScriptErrorKind::SetupExport,
                    })),
                    None => {
                        module_items.push(module_item);
                    }
                }

                continue;
            }
            ModuleItem::Stmt(stmt) => stmt,
        };

        let transformed = match stmt {
            Stmt::Expr(expr_stmt) => {
                let span = expr_stmt.span;

                let transform_macro_result = transform_script_setup_macro_expr(
                    ctx,
                    &expr_stmt.expr,
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

                    TransformMacroResult::Error(err) => {
                        errors.push(err);
                        None
                    }
                }
            }

            Stmt::Decl(decl) => {
                transform_decl_stmt(ctx, decl, &mut sfc_object_helper).map(Stmt::Decl)
            }

            // By default, just return the same statement
            _ => Some(stmt),
        };

        if let Some(transformed_stmt) = transformed {
            setup_body_stmts.push(transformed_stmt);
        }
    }

    // Post-process macros, e.g. merge models to `props` and `emits`
    postprocess_macros(&mut ctx.bindings_helper, &mut sfc_object_helper);

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
        module_items,
        sfc_object_helper,
        setup_fn,
    }
}

/// Analyzes the declaration in `script setup` context.
/// These are typically `var`/`let`/`const` declarations, function declarations, etc.
fn transform_decl_stmt(
    ctx: &mut TypeResolveContext,
    decl: Decl,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Decl> {
    /// Pushes the binding type and returns the same passed `Decl`
    macro_rules! push_return {
        ($binding: expr) => {
            ctx.bindings_helper.setup_bindings.push($binding);
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
                    let transform_macro_result =
                        transform_script_setup_macro_expr(ctx, init_expr, sfc_object_helper, true);

                    if let TransformMacroResult::ValidMacro(transformed_expr) =
                        transform_macro_result
                    {
                        // Macros always overwrite the RHS
                        var_declarator.init = transformed_expr;
                    } else if is_const && is_ident {
                        // Resolve only when this is a constant identifier.
                        // For destructures correct bindings are already assigned.
                        let rhs_type =
                            categorize_expr(init_expr, &ctx.bindings_helper.vue_resolved_imports);

                        enrich_binding_types(&mut collected_bindings, rhs_type, is_const, is_ident);
                    }
                }

                ctx.bindings_helper
                    .setup_bindings
                    .extend(collected_bindings.drain(..));
            }

            Some(Decl::Var(var_decl))
        }

        Decl::TsEnum(ref ts_enum) => {
            let is_all_literal = ts_enum
                .members
                .iter()
                .all(|m| m.init.as_ref().map_or(true, |e| is_static(&e)));

            // Ambient enums are also included, this is intentional
            // I am not sure about `const enum`s though
            push_return!(SetupBinding(
                ts_enum.id.sym.to_owned(),
                if is_all_literal {
                    BindingTypes::LiteralConst
                } else {
                    BindingTypes::SetupConst
                }
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

/// Returns an error span if non-type export is used
fn check_export(module_decl: &ModuleDecl) -> Option<Span> {
    match module_decl {
        ModuleDecl::ExportDecl(e) => match e.decl {
            Decl::Class(_)
            | Decl::Fn(_)
            | Decl::Var(_)
            | Decl::Using(_)
            | Decl::TsEnum(_)
            | Decl::TsModule(_) => Some(e.span),
            Decl::TsInterface(_) | Decl::TsTypeAlias(_) => None,
        },
        ModuleDecl::ExportNamed(e) if !e.type_only => Some(e.span),
        ModuleDecl::ExportDefaultDecl(e) => Some(e.span),
        ModuleDecl::ExportDefaultExpr(e) => Some(e.span),
        ModuleDecl::ExportAll(e) if !e.type_only => Some(e.span),
        ModuleDecl::TsExportAssignment(e) => Some(e.span),

        // ModuleDecl::Import(_) => todo!(),
        // ModuleDecl::TsImportEquals(_) => todo!(),
        // ModuleDecl::TsNamespaceExport(_) => todo!(),
        _ => None,
    }
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
    use crate::{
        error::{ScriptError, ScriptErrorKind, TransformError}, script::imports::process_imports, test_utils::parser::*, SetupBinding, TransformSfcContext
    };
    use fervid_core::{fervid_atom, BindingTypes, SfcScriptBlock};
    use swc_core::common::DUMMY_SP;

    use super::transform_and_record_script_setup;

    fn analyze_bindings(mut script_setup: SfcScriptBlock) -> Vec<SetupBinding> {
        let mut ctx = TransformSfcContext::anonymous();
        let mut errors = Vec::new();
        process_imports(&mut script_setup.content, &mut ctx.bindings_helper, true, &mut errors);
        transform_and_record_script_setup(&mut ctx, script_setup, &mut errors);

        ctx.bindings_helper.setup_bindings
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
            vec![SetupBinding(fervid_atom!("Baz"), BindingTypes::Imported)]
        );
    }

    #[test]
    fn with_typescript_with_generic_attribute() {
        // TODO Generics are not implemented yet
        // https://github.com/vuejs/core/blob/a41c5f1f4367a9f41bcdb8c4e02f54b2378e577d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L942
    }

    #[test]
    fn works_for_script_setup() {
        test_js_and_ts!(
            r"
            import { ref as r } from 'vue'
            defineProps({
                foo: String
            })

            const a = r(1)
            let b = 2
            const c = 3
            const { d } = someFoo()
            let { e } = someBar()
            ",
            vec![
                SetupBinding(fervid_atom!("foo"), BindingTypes::Props),
                SetupBinding(fervid_atom!("a"), BindingTypes::SetupRef),
                SetupBinding(fervid_atom!("b"), BindingTypes::SetupLet),
                SetupBinding(fervid_atom!("c"), BindingTypes::LiteralConst),
                SetupBinding(fervid_atom!("d"), BindingTypes::SetupMaybeRef),
                SetupBinding(fervid_atom!("e"), BindingTypes::SetupLet),
            ]
        );
    }

    // https://github.com/vuejs/core/blob/140a7681cc3bba22f55d97fd85a5eafe97a1230f/packages/compiler-sfc/__tests__/compileScript.spec.ts#L871-L890
    #[test]
    fn non_type_named_exports() {
        macro_rules! check {
            ($code: literal, $should_error: literal) => {
                let parsed = parse_typescript_module($code, 0, Default::default())
                    .expect("analyze_js expects the input to be parseable")
                    .0;

                let script_setup = SfcScriptBlock {
                    content: Box::new(parsed),
                    lang: fervid_core::SfcScriptLang::Typescript,
                    is_setup: true,
                    span: DUMMY_SP,
                };

                let mut ctx = TransformSfcContext::anonymous();
                let mut errors = Vec::new();
                transform_and_record_script_setup(&mut ctx, script_setup, &mut errors);

                if $should_error {
                    let error = errors.first().expect("Should have error");
                    assert!(matches!(
                        error,
                        TransformError::ScriptError(ScriptError {
                            kind: ScriptErrorKind::SetupExport,
                            ..
                        })
                    ));
                } else {
                    assert!(errors.is_empty());
                }
            };
        }

        macro_rules! expect_error {
            ($code: literal) => {
                check!($code, true);
            };
        }

        macro_rules! expect_no_error {
            ($code: literal) => {
                check!($code, false);
            };
        }

        expect_error!("export const a = 1");
        expect_error!("export * from './foo'");
        expect_error!(
            "
            const bar = 1
            export { bar as default }"
        );
        expect_no_error!("export type Foo = Bar | Baz");
        expect_no_error!("export interface Foo {}");
    }
}
