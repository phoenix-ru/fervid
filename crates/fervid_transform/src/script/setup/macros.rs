use fervid_core::VueImports;
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{
            ArrayLit, Bool, CallExpr, Callee, Expr, ExprOrSpread, ExprStmt, Ident, KeyValueProp,
            Lit, ObjectLit, Prop, PropName, PropOrSpread, Str,
        },
        atoms::{js_word, JsWord},
    },
};

use crate::{
    atoms::{
        DEFINE_EMITS, DEFINE_EXPOSE, DEFINE_MODEL, DEFINE_PROPS, EXPOSE_HELPER,
        MERGE_MODELS_HELPER, MODEL_VALUE, PROPS_HELPER, USE_MODEL_HELPER,
    },
    structs::{SfcDefineModel, SfcExportedObjectHelper},
};

pub fn transform_script_setup_macro_expr_stmt(
    expr_stmt: &ExprStmt,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<ExprStmt> {
    // `defineExpose` and `defineModel` actually generate something
    // https://play.vuejs.org/#eNp9kE1LxDAQhv/KmEtXWOphb8sqqBRU8AMVveRS2mnNmiYhk66F0v/uJGVXD8ueEt7nTfJkRnHtXL7rUazFhiqvXADC0LsraVTnrA8wgscGJmi87SDjaiaNNJU1FKCjFi4jX2R3qLWFT+t1fZadx0qNjTJYDM4SLsbUnRjM8aOtUS+yLi4fpeZbGW0uZgV+XCxFIH6kUW2+JWvYb5QGQIrKdk5p9M8uKJaQYg2JRFayw89DyoLvcbnPqy+svo/kWxpiJsWLR0K/QykOLJS+xTDj4u0JB94fIHv3mtsn4CuS1X10nGs3valZ+18v2d6nKSvTvlMxBDS0/1QUjc0p9aXgyd+e+Pqf7ipfpXPSTGL6BRH3n+Q=

    macro_rules! bail {
        () => {
            return Some(expr_stmt.to_owned());
        };
    }

    // Script setup macros are calls
    let Expr::Call(ref call_expr) = *expr_stmt.expr else {
        bail!();
    };

    // Callee is an expression
    let Callee::Expr(ref callee_expr) = call_expr.callee else {
        bail!();
    };

    let Expr::Ident(ref callee_ident) = **callee_expr else {
        bail!();
    };

    // We do a bit of a juggle here to use `string_cache`s fast comparisons
    let sym = &callee_ident.sym;
    if DEFINE_PROPS.eq(sym) {
        if let Some(arg0) = &call_expr.args.get(0) {
            // TODO Check if this was re-assigned before
            sfc_object_helper.props = Some(arg0.expr.to_owned());
        }

        None
    } else if DEFINE_EMITS.eq(sym) {
        if let Some(arg0) = &call_expr.args.get(0) {
            sfc_object_helper.emits = Some(arg0.expr.to_owned())
        }

        None
    } else if DEFINE_EXPOSE.eq(sym) {
        sfc_object_helper.exposes = true;

        // __expose
        let new_callee_ident = Ident {
            span: callee_ident.span,
            sym: EXPOSE_HELPER.to_owned(),
            optional: false,
        };

        // __expose(%call_expr.args%)
        Some(ExprStmt {
            span: expr_stmt.span,
            expr: Box::new(Expr::Call(CallExpr {
                span: call_expr.span,
                callee: Callee::Expr(Box::new(Expr::Ident(new_callee_ident))),
                args: call_expr.args.to_owned(),
                type_args: None,
            })),
        })
    } else if DEFINE_MODEL.eq(sym) {
        let define_model = read_define_model(&call_expr.args);
        let span = call_expr.span;

        // Add to imports
        sfc_object_helper.vue_imports |= VueImports::UseModel;

        let use_model_ident = Ident {
            span,
            sym: USE_MODEL_HELPER.to_owned(),
            optional: false,
        };

        let mut use_model_args =
            Vec::<ExprOrSpread>::with_capacity(if define_model.local { 3 } else { 2 });

        // __props
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
                            sym: js_word!("local"),
                            optional: false,
                        }),
                        value: Box::new(Expr::Lit(Lit::Bool(Bool { span, value: true }))),
                    })))],
                })),
            })
        }

        sfc_object_helper.models.push(define_model);

        // _useModel(__props, "model-name", %model options%)
        Some(ExprStmt {
            span: expr_stmt.span,
            expr: Box::new(Expr::Call(CallExpr {
                span,
                callee: Callee::Expr(Box::new(Expr::Ident(use_model_ident))),
                args: use_model_args,
                type_args: None,
            })),
        })
    } else {
        bail!();
    }
}

/// Mainly used to process `models` by adding them to `props` and `emits`
pub fn postprocess_macros(sfc_object_helper: &mut SfcExportedObjectHelper) {
    let len = sfc_object_helper.models.len();
    println!("Here {}", len);
    if len == 0 {
        return;
    }

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
                value: JsWord::from(model_update_evt_name),
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
                sfc_object_helper.vue_imports |= VueImports::MergeModels;
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
        None => {
            sfc_object_helper.props = Some(Box::new(Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: new_props,
            })))
        }
    }

    match sfc_object_helper.emits.take() {
        Some(mut existing_emits) => {
            // Try merging into an array if previous emits is an array
            if let Expr::Array(ref mut existing_emits_arr) = *existing_emits {
                existing_emits_arr.elems.extend(new_emits);

                sfc_object_helper.emits = Some(existing_emits);
            } else {
                // Use `mergeModels` otherwise
                sfc_object_helper.vue_imports |= VueImports::MergeModels;
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
        None => {
            sfc_object_helper.emits = Some(Box::new(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: new_emits,
            })))
        }
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
                return None
            };

            match key_value.key {
                PropName::Ident(ref ident) if ident.sym == js_word!("local") => {
                    Some(&key_value.value)
                }

                PropName::Str(ref s) if s.value == js_word!("local") => Some(&key_value.value),

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
