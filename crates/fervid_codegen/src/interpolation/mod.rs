use fervid_core::Interpolation;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, Ident},
};

use crate::{context::CodegenContext, imports::VueImports};

impl CodegenContext {
    pub fn generate_interpolation(
        &mut self,
        interpolation: &Interpolation
    ) -> (Expr, bool) {    
        // This is using a string with value if transformation failed
        // let (transformed, has_js_bindings) =
        //     // Polyfill
        //     parse_js(value)
        //         .and_then(|mut expr| {
        //             let has_js = transform_scoped(&mut expr, &self.scope_helper, scope_to_use);
        //             Ok((expr, has_js))
        //         })
        //         .unwrap_or_else(|_| {
        //             (
        //                 Box::new(Expr::Lit(Lit::Str(Str {
        //                     span,
        //                     value: JsWord::from(value),
        //                     raw: Some(Atom::from(value)),
        //                 }))),
        //                 false,
        //             )
        //         });

        // TODO Span
        let span = DUMMY_SP;

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
                    expr: interpolation.value.to_owned(),
                }],
                type_args: None,
            }),
            interpolation.patch_flag
        )
    }
}
