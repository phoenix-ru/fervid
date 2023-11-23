use fervid_core::fervid_atom;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{ArrowExpr, CallExpr, Expr, ExprOrSpread, Ident, Number},
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the `v-memo` directive.
    ///
    /// ## Example
    /// IN: `<div v-memo="[]"></div>`
    ///
    /// OUT:
    /// ```js
    /// _withMemo([], () => (_openBlock(), _createElementBlock("div")), _cache, 0)
    /// ```
    pub fn generate_v_memo(&mut self, memo_expr: Box<Expr>, item_render_expr: Box<Expr>) -> Expr {
        let cache_idx = self.allocate_next_cache_entry();

        // 1. Memo
        let memo = ExprOrSpread {
            spread: None,
            expr: memo_expr,
        };

        // 2. Transform `item_render_expr` to an arrow function
        let render_arrow = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Arrow(ArrowExpr {
                span: DUMMY_SP,
                params: vec![],
                body: Box::new(swc_core::ecma::ast::BlockStmtOrExpr::Expr(item_render_expr)),
                is_async: false,
                is_generator: false,
                type_params: None,
                return_type: None,
            })),
        };

        // 3. `_cache`
        let cache_ident = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: fervid_atom!("_cache"),
                optional: false,
            })),
        };

        // 4. Cache index
        let cache_idx_expr = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(swc_core::ecma::ast::Lit::Num(Number {
                span: DUMMY_SP,
                value: cache_idx as f64,
                raw: None,
            }))),
        };

        Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: swc_core::ecma::ast::Callee::Expr(Box::new(Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: self.get_and_add_import_ident(fervid_core::VueImports::WithMemo),
                optional: false,
            }))),
            args: vec![memo, render_arrow, cache_ident, cache_idx_expr],
            type_args: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_v_memo() {
        let mut ctx = CodegenContext::default();

        let res = ctx.generate_v_memo(js("[msg.value]"), js("_createElementVNode(\"div\")"));

        assert_eq!(
            crate::test_utils::to_str(res),
            "_withMemo([msg.value],()=>_createElementVNode(\"div\"),_cache,0)"
        );
    }
}
