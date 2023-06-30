use swc_core::{
    common::Span,
    ecma::{
        ast::{CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, Str},
        atoms::{Atom, JsWord},
    },
};

use crate::{context::CodegenContext, imports::VueImports, transform::transform_scoped};

impl CodegenContext {
    pub fn generate_dynamic_expression(
        &mut self,
        value: &str,
        scope_to_use: u32,
        span: Span,
    ) -> (Expr, bool) {
        // This is using a string with value if transformation failed
        let (transformed, has_js_bindings) =
            transform_scoped(value, &self.scope_helper, scope_to_use).unwrap_or_else(|| {
                (
                    Box::new(Expr::Lit(Lit::Str(Str {
                        span,
                        value: JsWord::from(value),
                        raw: Some(Atom::from(value)),
                    }))),
                    false,
                )
            });

        // toDisplayString(transformed)
        (
            Expr::Call(CallExpr {
                span,
                callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                    span,
                    sym: self.get_and_add_import_ident(VueImports::ToDisplayString),
                    optional: false,
                }))),
                args: vec![ExprOrSpread {
                    spread: None,
                    expr: transformed,
                }],
                type_args: None,
            }),
            has_js_bindings,
        )
    }
}
