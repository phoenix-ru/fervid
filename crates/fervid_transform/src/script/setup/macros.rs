use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{Callee, Expr, ExprStmt, Ident, KeyValueProp, PropOrSpread},
        atoms::JsWord,
    },
};

pub fn transform_script_setup_macro_expr_stmt(
    expr_stmt: &ExprStmt,
    sfc_fields: &mut Vec<PropOrSpread>,
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

    match callee_ident.sym.as_ref() {
        "defineProps" => {
            if let Some(arg0) = &call_expr.args.get(0) {
                // TODO use typed `sfc_fields`
                sfc_fields.push(PropOrSpread::Prop(Box::new(
                    swc_core::ecma::ast::Prop::KeyValue(KeyValueProp {
                        key: swc_core::ecma::ast::PropName::Ident(Ident {
                            span: DUMMY_SP,
                            sym: JsWord::from("props"),
                            optional: false,
                        }),
                        value: arg0.expr.to_owned(),
                    }),
                )))
            }
        }
        _ => {}
    }

    None
}
