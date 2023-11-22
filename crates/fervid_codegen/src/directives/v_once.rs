use fervid_core::{fervid_atom, VueImports};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, Number, AssignExpr, AssignOp, MemberExpr, ComputedPropName, ParenExpr, SeqExpr, BinExpr, BinaryOp},
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the complex cache structure for `v-once`.
    ///
    /// ## Example
    /// In:
    /// `<div v-once></div>`
    ///
    /// Out:
    /// ```js
    /// _cache[0] || (
    ///   _setBlockTracking(-1),
    ///   _cache[0] = _createElementVNode("div"),
    ///   _setBlockTracking(1),
    ///   _cache[0]
    /// )
    /// ```
    pub fn generate_v_once(&mut self, item_render_expr: Box<Expr>) -> Expr {
        // Prepare
        let cache_idx = self.allocate_next_cache_entry();
        let cache_ident = fervid_atom!("_cache");
        let set_block_tracking_ident = Box::new(Expr::Ident(Ident {
            span: DUMMY_SP,
            sym: self.get_and_add_import_ident(VueImports::SetBlockTracking),
            optional: false,
        }));

        // `_setBlockTracking($value)`
        macro_rules! set_block_tracking {
            ($value: literal, $ident: expr) => {
                Box::new(Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    callee: Callee::Expr($ident),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Lit(Lit::Num(Number {
                            span: DUMMY_SP,
                            value: $value,
                            raw: None,
                        }))),
                    }],
                    type_args: None,
                }))
            };
        }

        // 1. `_cache[cache_idx]`
        let cache_expr = Box::new(Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(Ident { span: DUMMY_SP, sym: cache_ident, optional: false })),
            prop: swc_core::ecma::ast::MemberProp::Computed(ComputedPropName {
                span: DUMMY_SP,
                expr: Box::new(Expr::Lit(Lit::Num(Number { span: DUMMY_SP, value: cache_idx as f64, raw: None }))),
            }),
        }));

        // 2. `_setBlockTracking(-1)`
        let decrement_tracking = set_block_tracking!(-1.0, set_block_tracking_ident.to_owned());
        
        // 3. `_cache[idx] = item_render_expr`
        let cache_assign = Box::new(Expr::Assign(AssignExpr {
            span: DUMMY_SP,
            op: AssignOp::Assign,
            left: swc_core::ecma::ast::PatOrExpr::Expr(cache_expr.to_owned()),
            right: item_render_expr,
        }));

        // 4. `_setBlockTracking(1)`
        let increment_tracking = set_block_tracking!(1.0, set_block_tracking_ident);

        // 5. Combine to
        // (
        //   _setBlockTracking(-1),
        //   _cache[0] = _createElementVNode("div"),
        //   _setBlockTracking(1),
        //   _cache[0]
        // )
        let parens_expr = Box::new(Expr::Paren(ParenExpr {
            span: DUMMY_SP,
            expr: Box::new(Expr::Seq(SeqExpr {
                span: DUMMY_SP,
                exprs: vec![
                    decrement_tracking,
                    cache_assign,
                    increment_tracking,
                    cache_expr.to_owned()
                ],
            })),
        }));

        // 6. Combine to the final form
        Expr::Bin(BinExpr {
            span: DUMMY_SP,
            op: BinaryOp::LogicalOr,
            left: cache_expr,
            right: parens_expr,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_v_once() {
        let mut ctx = CodegenContext::default();

        // Mock a render expression
        let item_render_expr = js("_createElementVNode(\"div\")");

        // First `v-once`
        let v_once_expr = ctx.generate_v_once(item_render_expr.to_owned());
        assert_eq!(
            crate::test_utils::to_str(v_once_expr),
            "_cache[0]||(_setBlockTracking(-1),_cache[0]=_createElementVNode(\"div\"),_setBlockTracking(1),_cache[0])"
        );

        // Second `v-once` with increased cache index
        let v_once_expr = ctx.generate_v_once(item_render_expr);
        assert_eq!(
            crate::test_utils::to_str(v_once_expr),
            "_cache[1]||(_setBlockTracking(-1),_cache[1]=_createElementVNode(\"div\"),_setBlockTracking(1),_cache[1])"
        );
    }
}
