use fervid_core::{VForDirective, VueImports};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{ArrowExpr, CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, Null, Number, Pat},
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates `(openBlock(true), createElementBlock(Fragment, null, renderList(<list>, (<item>) => (<expr>)), <patch flag>))`
    pub fn generate_v_for(&mut self, v_for: &VForDirective, item_render_expr: Expr) -> Expr {
        let span = DUMMY_SP; // TODO

        // Arrow function which renders each individual item
        let render_list_arrow = Expr::Arrow(ArrowExpr {
            span,
            params: vec![Pat::Expr(v_for.itervar.to_owned())],
            body: Box::new(swc_core::ecma::ast::BlockStmtOrExpr::Expr(Box::new(
                item_render_expr,
            ))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        });

        // `renderList` args
        // 1. List itself, which is `v_for.iterable`;
        // 2. Arrow function for each item, where argument is `v_for.iterator`
        //    and return is the passes `expr`;
        let mut render_list_args = Vec::with_capacity(2);
        render_list_args.push(ExprOrSpread {
            spread: None,
            expr: v_for.iterable.to_owned(),
        });
        render_list_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(render_list_arrow),
        });

        let render_list_call_expr = Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::RenderList),
                optional: false,
            }))),
            args: render_list_args,
            type_args: None,
        });

        // `_createElementBlock` args:
        // 1. `Fragment`;
        // 2. `null` (or `{ key: <number> }` in some rare cases);
        // 3. `renderList(<...>)`;
        // 4. Patch flag.
        let mut create_element_block_args = Vec::with_capacity(4);
        create_element_block_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::Fragment),
                optional: false,
            })),
        });
        create_element_block_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Null(Null { span }))),
        });
        create_element_block_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(render_list_call_expr),
        });
        create_element_block_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Num(Number {
                span,
                value: v_for.patch_flags.bits().into(),
                raw: None,
            }))),
        });

        let create_element_block = Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::CreateElementBlock),
                optional: false,
            }))),
            args: create_element_block_args,
            type_args: None,
        });

        self.wrap_in_open_block(create_element_block, span)
    }
}
