use fervid_core::{Interpolation, IntoIdent, VueImports};
use swc_core::ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread};

use crate::context::CodegenContext;

impl CodegenContext {
    pub fn generate_interpolation(&mut self, interpolation: &Interpolation) -> Expr {
        let span = interpolation.span;

        // toDisplayString(transformed)
        Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::ToDisplayString)
                    .into_ident_spanned(span),
            ))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: interpolation.value.to_owned(),
            }],
            type_args: None,
        })
    }
}
