use fervid_core::VueImports;
use swc_core::ecma::{
    ast::{
        Bool, CallExpr, Callee, Expr, ExprOrSpread, ExprStmt, Ident, KeyValueProp, Lit, ObjectLit,
        Prop, PropName, PropOrSpread, Str,
    },
    atoms::js_word,
};

use crate::{
    atoms::{
        DEFINE_EMITS, DEFINE_EXPOSE, DEFINE_MODEL, DEFINE_PROPS, EXPOSE_HELPER, MODEL_VALUE,
        PROPS_HELPER, USE_MODEL_HELPER,
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
                value: define_model.name,
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
