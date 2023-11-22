use fervid_core::{Interpolation, VueImports};
use swc_core::ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, Ident};

use crate::context::CodegenContext;

impl CodegenContext {
    pub fn generate_interpolation(&mut self, interpolation: &Interpolation) -> Expr {
        let span = interpolation.span;

        // toDisplayString(transformed)
        Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::ToDisplayString),
                optional: false,
            }))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: interpolation.value.to_owned(),
            }],
            type_args: None,
        })
    }
}
