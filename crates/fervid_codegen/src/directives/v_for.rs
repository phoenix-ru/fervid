use fervid_core::{fervid_atom, VForDirective, VueImports};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrowExpr, AssignExpr, AssignOp, BinExpr, BinaryOp, BindingIdent, BlockStmt,
        BlockStmtOrExpr, CallExpr, Callee, Decl, Expr, ExprOrSpread, ExprStmt, Ident, IfStmt, Lit,
        MemberExpr, Null, Number, Pat, PatOrExpr, ReturnStmt, Stmt, VarDecl, VarDeclKind,
        VarDeclarator,
    },
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates `(openBlock(true), createElementBlock(Fragment, null, renderList(<list>, (<item>) => (<expr>)), <patch flag>))`
    pub fn generate_v_for(&mut self, v_for: &VForDirective, item_render_expr: Box<Expr>) -> Expr {
        let span = v_for.span;

        // Arrow function which renders each individual item
        let render_list_arrow = Expr::Arrow(ArrowExpr {
            span,
            params: vec![Pat::Expr(v_for.itervar.to_owned())],
            body: Box::new(BlockStmtOrExpr::Expr(item_render_expr)),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        });

        // `_renderList` args
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

        // `_renderList(iterable, render_list_arrow)`
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

    /// Generates `v-for` in combination with `v-memo`.
    ///
    /// ## Example
    /// IN: `<div v-for="i in 3" v-memo="[]"></div>`
    ///
    /// OUT:
    /// ```js
    /// (_openBlock(), _createElementBlock(_Fragment, null, _renderList(3, (i, __, ___, _cached) => {
    ///   const _memo = ([])
    ///   if (_cached && _isMemoSame(_cached, _memo)) return _cached
    ///   const _item = (_openBlock(), _createElementBlock("div"))
    ///   _item.memo = _memo
    ///   return _item
    /// }, _cache, 0), 64 /* STABLE_FRAGMENT */))
    /// ```
    pub fn generate_v_for_memoized(
        &mut self,
        v_for: &VForDirective,
        item_render_expr: Box<Expr>,
        memo_expr: Box<Expr>,
    ) -> Expr {
        // Prepare
        let span = v_for.span;
        let cache_idx = self.allocate_next_cache_entry();

        // 1.1. `_renderList` first argument - iterable
        let render_list_iterable = ExprOrSpread {
            spread: None,
            expr: v_for.iterable.to_owned(),
        };

        // 1.2. `_renderList` second argument - the memoized arrow function
        let render_list_arrow = ExprOrSpread {
            spread: None,
            expr: self.generate_memoized_render_arrow(
                v_for.itervar.to_owned(),
                item_render_expr,
                memo_expr,
            ),
        };

        // 1.3. `_renderList` third argument - `_cache`
        let render_list_cache = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident {
                span,
                sym: fervid_atom!("_cache"),
                optional: false,
            })),
        };

        // 1.4. `_renderList` fourth argument - cache index
        let render_list_cache_idx = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Num(Number {
                span,
                value: cache_idx.into(),
                raw: None,
            }))),
        };

        // 1.5. Generate `_renderList` call
        let render_list = Box::new(Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::RenderList),
                optional: false,
            }))),
            args: vec![
                render_list_iterable,
                render_list_arrow,
                render_list_cache,
                render_list_cache_idx,
            ],
            type_args: None,
        }));

        // 2.1. `_Fragment`
        let fragment_ident = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::Fragment),
                optional: false,
            })),
        };

        // 2.2. `null` (or `{ key: <number> }` in some rare cases)
        let fragment_attrs = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Null(Null { span }))),
        };

        // 2.3. Fragment render function - just convert to ExprOrSpread
        let fragment_render = ExprOrSpread {
            spread: None,
            expr: render_list,
        };

        // 2.4. Fragment patch flag
        let fragment_patch_flag = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Num(Number {
                span,
                value: v_for.patch_flags.bits().into(),
                raw: None,
            }))),
        };

        // 2.5. Generate `_createElementBlock`
        let create_element_block = Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::CreateElementBlock),
                optional: false,
            }))),
            args: vec![
                fragment_ident,
                fragment_attrs,
                fragment_render,
                fragment_patch_flag,
            ],
            type_args: None,
        });

        self.wrap_in_open_block(create_element_block, span)
    }

    /// Generates the arrow function for [generate_v_for_memoized].
    ///
    /// ## Example
    /// IN: `<div v-for="i in 3" v-memo="[]"></div>`
    ///
    /// OUT:
    /// ```js
    /// (i, __, ___, _cached) => {
    ///   const _memo = ([])
    ///   if (_cached && _isMemoSame(_cached, _memo)) return _cached
    ///   const _item = (_openBlock(), _createElementBlock("div"))
    ///   _item.memo = _memo
    ///   return _item
    /// }
    /// ```
    fn generate_memoized_render_arrow(
        &mut self,
        itervar: Box<Expr>,
        item_render_expr: Box<Expr>,
        memo_expr: Box<Expr>,
    ) -> Box<Expr> {
        // `_cached`
        let cached_ident = Ident {
            span: DUMMY_SP,
            sym: fervid_atom!("_cached"),
            optional: false,
        };

        // `_memo`
        let memo_ident = Ident {
            span: DUMMY_SP,
            sym: fervid_atom!("_memo"),
            optional: false,
        };

        // Params for the function
        macro_rules! param {
            ($ident: literal) => {
                Pat::Ident(BindingIdent {
                    id: Ident {
                        span: DUMMY_SP,
                        sym: fervid_atom!($ident),
                        optional: false,
                    },
                    type_ann: None,
                })
            };
        }
        let arrow_params = vec![
            Pat::Expr(itervar),
            param!("__"),
            param!("___"),
            Pat::Ident(BindingIdent {
                id: cached_ident.to_owned(),
                type_ann: None,
            }),
        ];

        // `const _memo = ([])`
        let const_memo = Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: memo_ident.to_owned(),
                    type_ann: None,
                }),
                init: Some(memo_expr),
                definite: false,
            }],
        })));

        // `_isMemoSame(_cached, _memo)`
        let is_memo_same = Box::new(Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: self.get_and_add_import_ident(VueImports::IsMemoSame),
                optional: false,
            }))),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(cached_ident.to_owned())),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(memo_ident.to_owned())),
                },
            ],
            type_args: None,
        }));

        // `_cached && _isMemoSame(_cached, _memo)`
        let cache_cond = Box::new(Expr::Bin(BinExpr {
            span: DUMMY_SP,
            op: BinaryOp::LogicalAnd,
            left: Box::new(Expr::Ident(cached_ident.to_owned())),
            right: is_memo_same,
        }));

        // `if (_cached && _isMemoSame(_cached, _memo)) return _cached`
        let if_cached_return = Stmt::If(IfStmt {
            span: DUMMY_SP,
            test: cache_cond,
            cons: Box::new(Stmt::Return(ReturnStmt {
                span: DUMMY_SP,
                arg: Some(Box::new(Expr::Ident(cached_ident))),
            })),
            alt: None,
        });

        // `_item`
        let item_ident = Ident {
            span: DUMMY_SP,
            sym: fervid_atom!("_item"),
            optional: false,
        };

        // `const _item = item_render_expr`
        let const_item = Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: item_ident.to_owned(),
                    type_ann: None,
                }),
                init: Some(item_render_expr),
                definite: false,
            }],
        })));

        // `_item.memo = _memo`
        let item_set_memo = Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(Expr::Assign(AssignExpr {
                span: DUMMY_SP,
                op: AssignOp::Assign,
                left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
                    span: DUMMY_SP,
                    obj: Box::new(Expr::Ident(item_ident.to_owned())),
                    prop: swc_core::ecma::ast::MemberProp::Ident(Ident {
                        span: DUMMY_SP,
                        sym: fervid_atom!("memo"),
                        optional: false,
                    }),
                }))),
                right: Box::new(Expr::Ident(memo_ident)),
            })),
        });

        // `return _item`
        let return_item = Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg: Some(Box::new(Expr::Ident(item_ident))),
        });

        // Arrow body
        let arrow_body_stmts = vec![
            const_memo,
            if_cached_return,
            const_item,
            item_set_memo,
            return_item,
        ];

        Box::new(Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            params: arrow_params,
            body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                span: DUMMY_SP,
                stmts: arrow_body_stmts,
            })),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::PatchFlags;

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_v_for_memoized() {
        let mut ctx = CodegenContext::default();

        // `<div v-for="item in 3" v-memo="[msg]"></div>`
        let v_for = VForDirective {
            iterable: js("3"),
            itervar: js("item"),
            patch_flags: PatchFlags::StableFragment.into(),
            span: DUMMY_SP,
        };

        let res = ctx.generate_v_for_memoized(
            &v_for,
            js("_createElementVNode(\"div\")"),
            js("[msg.value]"),
        );

        assert_eq!(
            crate::test_utils::to_str(res),
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(3,(item,__,___,_cached)=>{const _memo=[msg.value];if(_cached&&_isMemoSame(_cached,_memo))return _cached;const _item=_createElementVNode(\"div\");_item.memo=_memo;return _item;},_cache,0),64))"
        );
    }
}
