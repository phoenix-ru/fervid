use fervid_core::{IntoIdent, VueImports};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrayLit, CallExpr, Callee, Decl, Expr, ExprOrSpread, Ident, Module, ModuleItem, ObjectLit,
        Pat, PropOrSpread, Stmt, VarDeclarator,
    },
};

use crate::{
    atoms::{
        DEFINE_EMITS, DEFINE_EXPOSE, DEFINE_MODEL, DEFINE_OPTIONS, DEFINE_PROPS, DEFINE_SLOTS,
        EXPOSE_HELPER, MERGE_MODELS_HELPER, WITH_DEFAULTS,
    },
    error::TransformError,
    script::{
        resolve_type::TypeResolveContext,
        setup::{
            define_emits::process_define_emits,
            define_model::process_define_model,
            define_options::process_define_options,
            define_props::{process_define_props, process_with_defaults},
            define_props_destructure::collect_props_destructure,
            define_slots::process_define_slots,
            utils::unwrap_ts_node_expr,
        },
    },
    structs::SfcExportedObjectHelper,
    SetupBinding, TransformSfcContext,
};

use super::define_model::postprocess_models;

pub struct CollectMacrosResult {
    pub has_type_only_macros: bool,
}

pub fn collect_macros(
    ctx: &mut TransformSfcContext,
    module: &Module,
    errors: &mut Vec<TransformError>,
) -> CollectMacrosResult {
    let mut result = CollectMacrosResult {
        has_type_only_macros: false,
    };

    // Check `defineProps` inside `withDefaults`
    // TODO Use a better approach
    fn is_type_only_with_defaults(call_expr: &CallExpr) -> bool {
        let Some(ExprOrSpread {
            expr: define_props_expr,
            spread: None,
        }) = call_expr.args.first()
        else {
            return false;
        };

        let Expr::Call(CallExpr {
            callee: Callee::Expr(callee_expr),
            type_args,
            ..
        }) = define_props_expr.as_ref()
        else {
            return false;
        };

        let Expr::Ident(callee_expr_ident) = callee_expr.as_ref() else {
            return false;
        };

        if callee_expr_ident.sym == *DEFINE_PROPS {
            type_args.is_some()
        } else {
            false
        }
    }

    for module_item in module.body.iter() {
        let ModuleItem::Stmt(module_stmt) = module_item else {
            continue;
        };

        match module_stmt {
            // E.g. `let foo = defineProps()`
            Stmt::Decl(Decl::Var(var_decl)) => {
                if var_decl.declare {
                    continue;
                }

                for var_decl_item in var_decl.decls.iter() {
                    let Some(ref init) = var_decl_item.init else {
                        continue;
                    };

                    let Expr::Call(ref call_expr) = init.as_ref() else {
                        continue;
                    };

                    let Callee::Expr(ref callee_expr) = call_expr.callee else {
                        continue;
                    };

                    let Expr::Ident(ref callee_ident) = callee_expr.as_ref() else {
                        continue;
                    };

                    let is_define_props = callee_ident.sym == *DEFINE_PROPS;
                    let is_define_emits = callee_ident.sym == *DEFINE_EMITS;
                    let is_with_defaults = callee_ident.sym == *WITH_DEFAULTS;
                    if !is_define_props && !is_define_emits && !is_with_defaults {
                        continue;
                    }

                    // Type-only marker
                    let has_type_args = if is_define_props || is_define_emits {
                        call_expr.type_args.is_some()
                    } else {
                        is_type_only_with_defaults(call_expr)
                    };

                    result.has_type_only_macros |= has_type_args;

                    // No other logic for `defineEmits`
                    if is_define_emits {
                        continue;
                    }

                    // Props destructure supports only object patterns
                    if let Pat::Object(ref obj_pat) = var_decl_item.name {
                        collect_props_destructure(ctx, obj_pat, errors)
                    }
                }
            }

            Stmt::Expr(expr_stmt) => {
                let Expr::Call(ref call_expr) = expr_stmt.expr.as_ref() else {
                    continue;
                };

                let Callee::Expr(ref callee_expr) = call_expr.callee else {
                    continue;
                };

                let Expr::Ident(ref callee_ident) = callee_expr.as_ref() else {
                    continue;
                };

                let is_define_props = callee_ident.sym == *DEFINE_PROPS;
                let is_define_emits = callee_ident.sym == *DEFINE_EMITS;
                let is_with_defaults = callee_ident.sym == *WITH_DEFAULTS;
                if !is_define_props && !is_define_emits && !is_with_defaults {
                    continue;
                }

                // Type-only marker
                let has_type_args = if is_define_props || is_define_emits {
                    call_expr.type_args.is_some()
                } else {
                    is_type_only_with_defaults(call_expr)
                };

                result.has_type_only_macros |= has_type_args;
            }

            _ => continue,
        };
    }

    result
}

pub enum TransformMacroResult {
    NotAMacro,
    ValidMacro(Option<Box<Expr>>),
    ValidMacroRewriteDeclarator(Option<Box<VarDeclarator>>),
    Error(TransformError),
}

pub struct VarDeclHelper<'a> {
    pub is_const: bool,
    pub lhs: &'a Pat,
    pub bindings: &'a mut Vec<SetupBinding>,
}

/// Tries to transform a Vue compiler macro.\
/// When `is_var_decl` is `true`, this function is guaranteed to return an `Expr`.
/// In case the macro transform does not return anything, an `Expr` containing `undefined` is returned instead.
///
/// See https://vuejs.org/api/sfc-script-setup.html#defineprops-defineemits
pub fn transform_script_setup_macro_expr(
    ctx: &mut TypeResolveContext,
    expr: &Expr,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    var_decl: Option<VarDeclHelper>,
    errors: &mut Vec<TransformError>,
) -> TransformMacroResult {
    // `defineExpose` and `defineModel` actually generate something
    // https://play.vuejs.org/#eNp9kE1LxDAQhv/KmEtXWOphb8sqqBRU8AMVveRS2mnNmiYhk66F0v/uJGVXD8ueEt7nTfJkRnHtXL7rUazFhiqvXADC0LsraVTnrA8wgscGJmi87SDjaiaNNJU1FKCjFi4jX2R3qLWFT+t1fZadx0qNjTJYDM4SLsbUnRjM8aOtUS+yLi4fpeZbGW0uZgV+XCxFIH6kUW2+JWvYb5QGQIrKdk5p9M8uKJaQYg2JRFayw89DyoLvcbnPqy+svo/kWxpiJsWLR0K/QykOLJS+xTDj4u0JB94fIHv3mtsn4CuS1X10nGs3valZ+18v2d6nKSvTvlMxBDS0/1QUjc0p9aXgyd+e+Pqf7ipfpXPSTGL6BRH3n+Q=

    let bindings_helper = &mut ctx.bindings_helper;

    /// Signify that this is not a macro
    macro_rules! bail {
        () => {
            return TransformMacroResult::NotAMacro;
        };
    }

    /// Signify that macro is valid.
    /// Value provided is the substitution that will be made instead of a macro call expression.
    /// `None` means macro does not produce code in-place, e.g. `defineOptions(/*...*/)` produces nothing.
    macro_rules! valid_macro {
        // TODO Handle `None` + `is_var_decl == true` case because it clears var declaration RHS (init expr).
        ($return_value: expr) => {
            TransformMacroResult::ValidMacro($return_value)
        };
    }

    // Strip TS to get to the actual expression
    let expr = unwrap_ts_node_expr(expr);

    // Script setup macros are calls
    let Expr::Call(ref call_expr) = *expr else {
        bail!();
    };

    // Callee is an expression
    let Callee::Expr(ref callee_expr) = call_expr.callee else {
        bail!();
    };

    let Expr::Ident(ref callee_ident) = **callee_expr else {
        bail!();
    };

    // TODO We can also strip out `onMounted` and `onUnmounted` for SSR here
    // Not only that, but we can remove any DOM-related listeners in template,
    // e.g. `<button @click="onClick">`. This should apply to all Node::Element (not components).

    // We do a bit of a juggle here to use `string_cache`s fast comparisons
    let sym = &callee_ident.sym;
    let span = call_expr.span;
    if DEFINE_PROPS.eq(sym) {
        process_define_props(ctx, call_expr, var_decl, sfc_object_helper, errors)
    } else if WITH_DEFAULTS.eq(sym) {
        process_with_defaults(ctx, call_expr, var_decl, sfc_object_helper, errors)
    } else if DEFINE_EMITS.eq(sym) {
        process_define_emits(ctx, call_expr, var_decl, sfc_object_helper, errors)
    } else if DEFINE_EXPOSE.eq(sym) {
        sfc_object_helper.is_setup_expose_referenced = true;

        // __expose
        let new_callee_ident = Ident {
            span: callee_ident.span,
            ctxt: Default::default(),
            sym: EXPOSE_HELPER.to_owned(),
            optional: false,
        };

        // __expose(%call_expr.args%)
        valid_macro!(Some(Box::new(Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(new_callee_ident))),
            args: call_expr.args.to_owned(),
            type_args: None,
        }))))
    } else if DEFINE_MODEL.eq(sym) {
        process_define_model(call_expr, var_decl, sfc_object_helper, bindings_helper)
    } else if DEFINE_SLOTS.eq(sym) {
        process_define_slots(
            call_expr,
            var_decl.is_some(),
            sfc_object_helper,
            bindings_helper,
        )
    } else if DEFINE_OPTIONS.eq(sym) {
        process_define_options(call_expr, var_decl.is_some(), sfc_object_helper, errors)
    } else {
        TransformMacroResult::NotAMacro
    }
}

/// Mainly used to process `models` by adding them to `props` and `emits`
pub fn postprocess_macros(
    ctx: &mut TypeResolveContext,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) {
    // Capacity is twice the length because for each model we push both the prop and modelModifiers
    let len = sfc_object_helper.models.len();
    let mut new_props = Vec::<PropOrSpread>::with_capacity(len * 2);
    let mut new_emits = Vec::<Option<ExprOrSpread>>::with_capacity(len);

    postprocess_models(
        ctx,
        &mut sfc_object_helper.models,
        &mut new_props,
        &mut new_emits,
    );

    // Take existing props if the new ones have something
    let existing_props = if new_props.is_empty() {
        None
    } else {
        sfc_object_helper.props.take()
    };

    match existing_props {
        Some(mut existing_props) => {
            // Try merging into an object if previous props is an object
            if let Expr::Object(ref mut existing_props_obj) = *existing_props {
                existing_props_obj.props.extend(new_props);

                sfc_object_helper.props = Some(existing_props);
            } else {
                // Use `mergeModels` otherwise
                ctx.bindings_helper.vue_imports |= VueImports::MergeModels;
                let merge_models_ident = MERGE_MODELS_HELPER.to_owned();

                let new_props = Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(merge_models_ident.into_ident()))),
                    args: vec![
                        ExprOrSpread {
                            spread: None,
                            expr: existing_props,
                        },
                        ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Object(ObjectLit {
                                span: DUMMY_SP,
                                props: new_props,
                            })),
                        },
                    ],
                    type_args: None,
                });

                sfc_object_helper.props = Some(Box::new(new_props));
            };
        }
        None if !new_props.is_empty() => {
            sfc_object_helper.props = Some(Box::new(Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: new_props,
            })))
        }
        _ => {}
    }

    // Take existing emits if the new one has something
    let existing_emits = if new_emits.is_empty() {
        None
    } else {
        sfc_object_helper.emits.take()
    };

    match existing_emits {
        Some(mut existing_emits) => {
            // Try merging into an array if previous emits is an array
            if let Expr::Array(ref mut existing_emits_arr) = *existing_emits {
                existing_emits_arr.elems.extend(new_emits);

                sfc_object_helper.emits = Some(existing_emits);
            } else {
                // Use `mergeModels` otherwise
                ctx.bindings_helper.vue_imports |= VueImports::MergeModels;
                let merge_models_ident = MERGE_MODELS_HELPER.to_owned();

                let new_emits = Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(merge_models_ident.into_ident()))),
                    args: vec![
                        ExprOrSpread {
                            spread: None,
                            expr: existing_emits,
                        },
                        ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Array(ArrayLit {
                                span: DUMMY_SP,
                                elems: new_emits,
                            })),
                        },
                    ],
                    type_args: None,
                });

                sfc_object_helper.emits = Some(Box::new(new_emits));
            };
        }
        None if !new_emits.is_empty() => {
            sfc_object_helper.emits = Some(Box::new(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: new_emits,
            })))
        }
        _ => {}
    }
}
