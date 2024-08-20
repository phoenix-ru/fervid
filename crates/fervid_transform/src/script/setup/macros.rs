use fervid_core::{fervid_atom, FervidAtom, VueImports};
use fxhash::FxHashSet;
use itertools::{Either, Itertools};
use swc_core::{
    common::{Spanned, DUMMY_SP},
    ecma::ast::{
        ArrayLit, Bool, CallExpr, Callee, Expr, ExprOrSpread, Ident, KeyValueProp, Lit, ObjectLit,
        Prop, PropName, PropOrSpread, Str, TsFnOrConstructorType, TsFnParam, TsLit, TsType,
    },
};

use crate::{
    atoms::{
        DEFINE_EMITS, DEFINE_EXPOSE, DEFINE_MODEL, DEFINE_OPTIONS, DEFINE_PROPS, DEFINE_SLOTS,
        EMIT_HELPER, EXPOSE_HELPER, MERGE_MODELS_HELPER, MODEL_VALUE, PROPS_HELPER,
        USE_MODEL_HELPER, WITH_DEFAULTS,
    },
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::{
        resolve_type::{
            resolve_type_elements, resolve_union_type, ResolvedElements, TypeResolveContext,
        },
        setup::define_props::{process_define_props, process_with_defaults},
    },
    structs::{SfcDefineModel, SfcExportedObjectHelper},
    BindingsHelper,
};

pub enum TransformMacroResult {
    NotAMacro,
    ValidMacro(Option<Box<Expr>>),
    Error(TransformError),
}

/// Tries to transform a Vue compiler macro.\
/// When `is_var_decl` is `true`, this function is guaranteed to return an `Expr`.
/// In case the macro transform does not return anything, an `Expr` containing `undefined` is returned instead.
///
/// See https://vuejs.org/api/sfc-script-setup.html#defineprops-defineemits
pub fn transform_script_setup_macro_expr(
    ctx: &mut TypeResolveContext,
    expr: &Expr,
    bindings_helper: &mut BindingsHelper,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    is_var_decl: bool,
) -> TransformMacroResult {
    // `defineExpose` and `defineModel` actually generate something
    // https://play.vuejs.org/#eNp9kE1LxDAQhv/KmEtXWOphb8sqqBRU8AMVveRS2mnNmiYhk66F0v/uJGVXD8ueEt7nTfJkRnHtXL7rUazFhiqvXADC0LsraVTnrA8wgscGJmi87SDjaiaNNJU1FKCjFi4jX2R3qLWFT+t1fZadx0qNjTJYDM4SLsbUnRjM8aOtUS+yLi4fpeZbGW0uZgV+XCxFIH6kUW2+JWvYb5QGQIrKdk5p9M8uKJaQYg2JRFayw89DyoLvcbnPqy+svo/kWxpiJsWLR0K/QykOLJS+xTDj4u0JB94fIHv3mtsn4CuS1X10nGs3valZ+18v2d6nKSvTvlMxBDS0/1QUjc0p9aXgyd+e+Pqf7ipfpXPSTGL6BRH3n+Q=

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
        process_define_props(ctx, call_expr, is_var_decl, sfc_object_helper, bindings_helper)
    } else if WITH_DEFAULTS.eq(sym) {
        process_with_defaults(ctx, call_expr, is_var_decl, sfc_object_helper, bindings_helper)
    } else if DEFINE_EMITS.eq(sym) {
        // Validation: duplicate call
        if sfc_object_helper.emits.is_some() {
            return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
                span: call_expr.span,
                kind: ScriptErrorKind::DuplicateDefineEmits,
            }));
        }

        // Validation: both runtime and types
        if !call_expr.args.is_empty() && call_expr.type_args.is_some() {
            return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
                span: call_expr.span,
                kind: ScriptErrorKind::DefineEmitsTypeAndNonTypeArguments,
            }));
        }

        if let Some(arg0) = &call_expr.args.get(0) {
            sfc_object_helper.emits = Some(arg0.expr.to_owned())
        } else if let Some(ref type_args) = call_expr.type_args {
            let Some(ts_type) = type_args.params.first() else {
                return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
                    span: type_args.span,
                    kind: ScriptErrorKind::DefineEmitsMalformed,
                }));
            };

            let runtime_emits = match extract_runtime_emits(ctx, &ts_type) {
                Ok(v) => v,
                Err(e) => return TransformMacroResult::Error(TransformError::ScriptError(e)),
            };

            sfc_object_helper.emits = Some(Box::new(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: runtime_emits
                    .into_iter()
                    .map(|it| {
                        Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Lit(Lit::Str(Str {
                                span: DUMMY_SP,
                                value: it,
                                raw: None,
                            }))),
                        })
                    })
                    .collect_vec(),
            })))
        }

        // TODO Process type declaration

        // Return `__emits` when in var mode
        if is_var_decl {
            sfc_object_helper.is_setup_emit_referenced = true;

            valid_macro!(Some(Box::new(Expr::Ident(Ident {
                span,
                sym: EMIT_HELPER.to_owned(),
                optional: false,
            }))))
        } else {
            valid_macro!(None)
        }
    } else if DEFINE_EXPOSE.eq(sym) {
        sfc_object_helper.is_setup_expose_referenced = true;

        // __expose
        let new_callee_ident = Ident {
            span: callee_ident.span,
            sym: EXPOSE_HELPER.to_owned(),
            optional: false,
        };

        // __expose(%call_expr.args%)
        valid_macro!(Some(Box::new(Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(new_callee_ident))),
            args: call_expr.args.to_owned(),
            type_args: None,
        }))))
    } else if DEFINE_MODEL.eq(sym) {
        let define_model = read_define_model(&call_expr.args);

        // Add to imports
        bindings_helper.vue_imports |= VueImports::UseModel;

        // TODO Add model identifier as a binding (when `is_var_decl == true`)

        let use_model_ident = Ident {
            span,
            sym: USE_MODEL_HELPER.to_owned(),
            optional: false,
        };

        let mut use_model_args =
            Vec::<ExprOrSpread>::with_capacity(if define_model.local { 3 } else { 2 });

        // __props
        sfc_object_helper.is_setup_props_referenced = true;
        use_model_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident {
                span,
                sym: PROPS_HELPER.to_owned(),
                optional: false,
            })),
        });

        // "model-name"
        use_model_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Str(Str {
                span,
                value: define_model.name.to_owned(),
                raw: None,
            }))),
        });

        // `{ local: true }` if needed
        if define_model.local {
            use_model_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(ObjectLit {
                    span,
                    props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                        key: PropName::Ident(Ident {
                            span,
                            sym: fervid_atom!("local"),
                            optional: false,
                        }),
                        value: Box::new(Expr::Lit(Lit::Bool(Bool { span, value: true }))),
                    })))],
                })),
            })
        }

        sfc_object_helper.models.push(define_model);

        // _useModel(__props, "model-name", %model options%)
        valid_macro!(Some(Box::new(Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(use_model_ident))),
            args: use_model_args,
            type_args: None,
        }))))
    } else if DEFINE_SLOTS.eq(sym) {
        // Without a variable to bind to this macro means nothing
        if !is_var_decl {
            bail!();
        }

        // Add to imports and get the identifier
        bindings_helper.vue_imports |= VueImports::UseSlots;
        let use_slots_ident = Ident {
            span,
            sym: VueImports::UseSlots.as_atom(),
            optional: false,
        };

        // Add a binding
        // TODO Integrate closer with `categorize_var_declarator`

        // _useSlots()
        valid_macro!(Some(Box::new(Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(use_slots_ident))),
            args: vec![],
            type_args: None,
        }))))
    } else if DEFINE_OPTIONS.eq(sym) {
        // A variable is not a correct usage
        if is_var_decl {
            bail!();
        }

        // `defineOptions()` without arguments
        let Some(ExprOrSpread { spread: None, expr }) = call_expr.args.get(0) else {
            return valid_macro!(None);
        };

        // Try to take out object, otherwise just use spread
        let Expr::Object(ref options_object) = **expr else {
            sfc_object_helper.untyped_fields.push(PropOrSpread::Spread(
                swc_core::ecma::ast::SpreadElement {
                    dot3_token: DUMMY_SP,
                    expr: expr.to_owned(),
                },
            ));
            return valid_macro!(None);
        };

        // Copy the fields
        sfc_object_helper
            .untyped_fields
            .extend(options_object.props.iter().cloned());

        valid_macro!(None)
    } else {
        TransformMacroResult::NotAMacro
    }
}

/// Mainly used to process `models` by adding them to `props` and `emits`
pub fn postprocess_macros(
    bindings_helper: &mut BindingsHelper,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) {
    let len = sfc_object_helper.models.len();
    let mut new_props = Vec::<PropOrSpread>::with_capacity(len);
    let mut new_emits = Vec::<Option<ExprOrSpread>>::with_capacity(len);

    for model in sfc_object_helper.models.drain(..) {
        let model_value: Box<Expr> = match model.options {
            Some(options) => options.expr,
            None => Box::new(Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: vec![],
            })),
        };

        let mut model_update_evt_name = String::with_capacity("update:".len() + model.name.len());
        model_update_evt_name.push_str("update:");
        model_update_evt_name.push_str(&model.name);

        // Push a string literal into emits
        new_emits.push(Some(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: FervidAtom::from(model_update_evt_name),
                raw: None,
            }))),
        }));

        // Push an options object (or expr) into props
        new_props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Str(Str {
                span: DUMMY_SP,
                value: model.name,
                raw: None,
            }),
            value: model_value,
        }))));
    }

    match sfc_object_helper.props.take() {
        Some(mut existing_props) => {
            // Try merging into an object if previous props is an object
            if let Expr::Object(ref mut existing_props_obj) = *existing_props {
                existing_props_obj.props.extend(new_props);

                sfc_object_helper.props = Some(existing_props);
            } else {
                // Use `mergeModels` otherwise
                bindings_helper.vue_imports |= VueImports::MergeModels;
                let merge_models_ident = MERGE_MODELS_HELPER.to_owned();

                let new_props = Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                        span: DUMMY_SP,
                        sym: merge_models_ident,
                        optional: false,
                    }))),
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

    match sfc_object_helper.emits.take() {
        Some(mut existing_emits) => {
            // Try merging into an array if previous emits is an array
            if let Expr::Array(ref mut existing_emits_arr) = *existing_emits {
                existing_emits_arr.elems.extend(new_emits);

                sfc_object_helper.emits = Some(existing_emits);
            } else {
                // Use `mergeModels` otherwise
                bindings_helper.vue_imports |= VueImports::MergeModels;
                let merge_models_ident = MERGE_MODELS_HELPER.to_owned();

                let new_emits = Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                        span: DUMMY_SP,
                        sym: merge_models_ident,
                        optional: false,
                    }))),
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

/// Processes `defineModel`
fn read_define_model(macro_args: &[ExprOrSpread]) -> SfcDefineModel {
    // 1st arg - model name (string) or model options (object)
    let first_arg = macro_args.get(0);

    // 2nd arg - model options (object)
    let second_arg = macro_args.get(1);

    // Get name. It may be a first argument, or may be omitted altogether (defaults to `modelValue`)
    let (name, is_first_arg_name) = match first_arg {
        Some(ExprOrSpread { spread: None, expr }) => match **expr {
            Expr::Lit(Lit::Str(ref name)) => (name.value.to_owned(), true),
            _ => (MODEL_VALUE.to_owned(), false),
        },

        _ => (MODEL_VALUE.to_owned(), false),
    };

    let options: Option<&ExprOrSpread> = if is_first_arg_name {
        second_arg
    } else {
        first_arg
    };

    // Check if options is an object, we'll need `local` option from it
    let local = is_local(options);

    SfcDefineModel {
        name,
        local,
        options: options.map(|o| Box::new(o.to_owned())),
    }
}

/// Dig into options and find `local` field in the object with a boolean value.
/// If property is not found or `options` is not a proper object, `false` is returned.
fn is_local(options: Option<&ExprOrSpread>) -> bool {
    let Some(ExprOrSpread { spread: None, expr }) = options else {
        return false;
    };

    let Expr::Object(ref obj) = **expr else {
        return false;
    };

    let local_prop_value = obj.props.iter().find_map(|prop| match prop {
        PropOrSpread::Prop(prop) => {
            let Prop::KeyValue(ref key_value) = **prop else {
                return None;
            };

            match key_value.key {
                PropName::Ident(ref ident) if ident.sym == fervid_atom!("local") => {
                    Some(&key_value.value)
                }

                PropName::Str(ref s) if s.value == fervid_atom!("local") => Some(&key_value.value),

                _ => None,
            }
        }
        _ => None,
    });

    let Some(local_prop_value) = local_prop_value else {
        return false;
    };

    let Expr::Lit(Lit::Bool(ref local_bool)) = **local_prop_value else {
        return false;
    };

    local_bool.value
}

/// Extracts runtime emits from type-only `defineEmits` declaration
/// Adapted from https://github.com/vuejs/core/blob/0ac0f2e338f6f8f0bea7237db539c68bfafb88ae/packages/compiler-sfc/src/script/defineEmits.ts#L73-L103
fn extract_runtime_emits(
    ctx: &mut TypeResolveContext,
    type_arg: &TsType,
) -> Result<FxHashSet<FervidAtom>, ScriptError> {
    let mut emits = FxHashSet::<FervidAtom>::default();

    // Handle cases like `defineEmits<(e: 'foo' | 'bar') => void>()`
    if let TsType::TsFnOrConstructorType(TsFnOrConstructorType::TsFnType(ref ts_fn_type)) = type_arg
    {
        // Expect first param in fn, e.g. `e: 'foo' | 'bar'` in example above
        let Some(first_fn_param) = ts_fn_type.params.first() else {
            return Err(ScriptError {
                span: ts_fn_type.span,
                kind: ScriptErrorKind::DefineEmitsMalformed,
            });
        };

        extract_event_names(ctx, first_fn_param, &mut emits);

        return Ok(emits);
    }

    let ResolvedElements { props, calls } = resolve_type_elements(ctx, type_arg)?;

    let mut has_property = false;
    for key in props.into_keys() {
        emits.insert(key);
        has_property = true;
    }

    if !calls.is_empty() {
        if has_property {
            return Err(ScriptError {
                kind: ScriptErrorKind::DefineEmitsMixedCallAndPropertySyntax,
                span: type_arg.span(),
            });
        }

        for call in calls {
            let (params, span) = match call {
                Either::Left(l) => (l.params, l.span),
                Either::Right(r) => (r.params, r.span),
            };

            let Some(first_param) = params.first() else {
                return Err(ScriptError {
                    span,
                    kind: ScriptErrorKind::ResolveTypeMissingTypeParam,
                });
            };
            extract_event_names(ctx, first_param, &mut emits);
        }
    }

    return Ok(emits);
}

/// Adapted from https://github.com/vuejs/core/blob/0ac0f2e338f6f8f0bea7237db539c68bfafb88ae/packages/compiler-sfc/src/script/defineEmits.ts#L105-L128
fn extract_event_names(
    ctx: &mut TypeResolveContext,
    event_name: &TsFnParam,
    emits: &mut FxHashSet<FervidAtom>,
) {
    let TsFnParam::Ident(ident) = event_name else {
        return;
    };

    let Some(ref type_annotation) = ident.type_ann else {
        return;
    };

    let scope = ctx.scope.clone();

    let types = resolve_union_type(ctx, &type_annotation.type_ann, &scope);
    for ts_type in types {
        if let TsType::TsLitType(ts_lit_type) = ts_type {
            // No UnaryExpression
            match ts_lit_type.lit {
                TsLit::Number(ref n) => {
                    emits.insert(FervidAtom::from(n.value.to_string()));
                }
                TsLit::Str(ref s) => {
                    emits.insert(s.value.to_owned());
                }
                TsLit::Bool(ref b) => {
                    emits.insert(FervidAtom::from(b.value.to_string()));
                }
                TsLit::BigInt(ref big_int) => {
                    emits.insert(FervidAtom::from(big_int.value.to_string()));
                }
                TsLit::Tpl(_) => {}
            }
        }
    }
}
