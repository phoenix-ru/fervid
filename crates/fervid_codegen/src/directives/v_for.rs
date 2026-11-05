use fervid_core::{
    ForCodegenNodeMemo, ForNode, ForParseResult, IntoIdent, Node, PatchFlags, VForDirective,
    VueImports, fervid_atom,
};
use fervid_transform::template::utils::maybe_parenthesize;
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{
        ArrowExpr, AssignExpr, AssignOp, AssignTarget, BinExpr, BinaryOp, BindingIdent, BlockStmt,
        BlockStmtOrExpr, CallExpr, Callee, Decl, Expr, ExprOrSpread, ExprStmt, Ident, IdentName,
        IfStmt, Lit, MemberExpr, MemberProp, Null, Number, ObjectLit, Pat, Prop, PropName,
        PropOrSpread, ReturnStmt, SimpleAssignTarget, Stmt, VarDecl, VarDeclKind, VarDeclarator,
    },
};

use crate::CodegenContext;

pub(crate) fn create_for_loop_params(result: &ForParseResult, minimum_len: usize) -> Vec<Pat> {
    let params_len = if result.index.is_some() {
        3
    } else if result.key.is_some() {
        2
    } else {
        1
    }
    .max(minimum_len);

    (0..params_len)
        .map(|index| {
            let param = match index {
                0 => Some(&result.value),
                1 => result.key.as_ref(),
                2 => result.index.as_ref(),
                _ => None,
            };

            param.map_or_else(
                || {
                    Pat::Ident(BindingIdent {
                        id: Ident {
                            span: DUMMY_SP,
                            ctxt: Default::default(),
                            sym: "_".repeat(index + 1).into(),
                            optional: false,
                        },
                        type_ann: None,
                    })
                },
                |param| Pat::Expr(param.to_owned()),
            )
        })
        .collect()
}

impl CodegenContext {
    pub fn generate_for_node(&mut self, for_node: &ForNode) -> Expr {
        // TODO: Rework properly
        let codegen_node = for_node
            .codegen_node
            .as_deref()
            .expect("ForNode must have codegen metadata");
        let span = for_node.span;
        let is_stable = codegen_node
            .patch_flags
            .contains(PatchFlags::StableFragment);

        // 1.1. One Element child can be generated as-is (likely into VNodeCall),
        // otherwise wrap it with Fragment
        let item_render_expr = Box::new(if let [Node::Element(_)] = for_node.children.as_slice() {
            self.generate_node(&for_node.children[0], !is_stable)
        } else {
            self.generate_for_fragment(&for_node.children, codegen_node.key.as_deref(), span)
        });

        // 2. Generate `renderList`. Two cases are possible here:
        // a. Normal (e.g. `<div v-for="item in list" :key="item.key" />`
        // _renderList(_ctx.list, (item) => {
        //   return (_openBlock(), _createElementBlock("div", {
        //     key: item.key
        //   }))
        // })
        //
        // b. Memoized (e.g. `<div v-for="item in list" v-memo="[]" :key="item.key" />`)
        // _renderList(_ctx.list, (item, __, ___, _cached) => {
        //   const _memo = ([])
        //   if (_cached && _cached.el && _cached.key === item.key && _isMemoSame(_cached, _memo)) return _cached
        //   const _item = (_openBlock(), _createElementBlock("div", { key: item.key }))
        //   _item.memo = _memo
        //   return _item
        // }, _cache, 4)

        // 2.1. `_renderList` first argument - iterable
        let render_list_iterable = ExprOrSpread {
            spread: None,
            expr: for_node.parse_result.source.to_owned(),
        };

        // 2.2. `_renderList` second argument - render function or memoized arrow function
        let (render_list_arrow, memo_cache_args) = if let Some(ref memo) = codegen_node.memo {
            // Memoized
            let memoized_arrow = self.generate_memoized_render_arrow(
                create_for_loop_params(&for_node.parse_result, 3),
                item_render_expr,
                memo.to_owned(),
            );

            // Memoized renderList stores previous VNodes in `_cache[index]`.
            // `_renderList(source, arrow, _cache, index)`
            let index = self.allocate_next_cache_entry();
            let cache = expr_arg(Expr::Ident(fervid_atom!("_cache").into_ident()));
            let cache_index = expr_arg(Expr::Lit(Lit::Num(Number {
                span,
                value: index.into(),
                raw: None,
            })));

            (
                ExprOrSpread::from(memoized_arrow),
                Some([cache, cache_index]),
            )
        } else {
            // Plain render function
            (
                ExprOrSpread::from(Expr::Arrow(ArrowExpr {
                    span,
                    ctxt: Default::default(),
                    params: create_for_loop_params(&for_node.parse_result, 1),
                    body: Box::new(BlockStmtOrExpr::Expr(item_render_expr)),
                    is_async: false,
                    is_generator: false,
                    type_params: None,
                    return_type: None,
                })),
                None,
            )
        };

        // 2.3. `_renderList` arguments:
        // Plain:    `(source, arrow)`
        // Memoized: `(source, arrow, _cache, index)`
        let mut render_list_args = vec![render_list_iterable, render_list_arrow];
        if let Some(memo_cache_args) = memo_cache_args {
            render_list_args.extend(memo_cache_args);
        }

        // 2.4. Build `renderList` itself
        let render_list = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::RenderList)
                    .into_ident_spanned(span),
            ))),
            args: render_list_args,
            type_args: None,
        });

        // 3. Build the for loop Fragment.
        // 3.1. Build the branch key when v-for is used together with v-if (`{ key: <number> }`)
        let branch_key = if let Some(key) = codegen_node.branch_key {
            expr_arg(key_object(Expr::from(key), span))
        } else {
            expr_arg(Expr::Lit(Lit::Null(Null { span })))
        };

        // 3.2. Build the Fragment itself.
        // Note: this is separate from Fragment which is built for multi-child for
        let fragment = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::CreateElementBlock)
                    .into_ident_spanned(span),
            ))),
            args: vec![
                expr_arg(Expr::Ident(
                    self.get_and_add_import_ident(VueImports::Fragment)
                        .into_ident_spanned(span),
                )),
                branch_key,
                expr_arg(render_list),
                expr_arg(Expr::Lit(Lit::Num(Number {
                    span,
                    value: codegen_node.patch_flags.bits().into(),
                    raw: None,
                }))),
            ],
            type_args: None,
        });

        // 4. Wrap in `openBlock`
        let value =
            self.wrap_in_open_block_with_tracking(fragment, span, codegen_node.disable_tracking);

        // 5. Cache if needed
        if codegen_node.cache.is_empty() {
            value
        } else {
            let index = self.allocate_next_cache_entry();
            self.wrap_cache_expression(index, value, codegen_node.cache)
        }
    }

    fn generate_for_fragment(&mut self, children: &[Node], key: Option<&Expr>, span: Span) -> Expr {
        let mut generated_children = Vec::new();
        self.generate_node_sequence(
            &mut children.iter(),
            &mut generated_children,
            children.len(),
            false,
        );

        let props = key.map_or_else(
            || Expr::Lit(Lit::Null(Null { span })),
            |key| key_object(key.clone(), span),
        );
        let fragment = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::CreateElementBlock)
                    .into_ident_spanned(span),
            ))),
            args: vec![
                expr_arg(Expr::Ident(
                    self.get_and_add_import_ident(VueImports::Fragment)
                        .into_ident_spanned(span),
                )),
                expr_arg(props),
                expr_arg(Expr::Array(swc_core::ecma::ast::ArrayLit {
                    span,
                    elems: generated_children
                        .into_iter()
                        .map(|child| Some(expr_arg(child)))
                        .collect(),
                })),
                expr_arg(Expr::Lit(Lit::Num(Number {
                    span,
                    value: fervid_core::PatchFlagsSet::from(PatchFlags::StableFragment)
                        .bits()
                        .into(),
                    raw: None,
                }))),
            ],
            type_args: None,
        });

        self.wrap_in_open_block(fragment, span)
    }

    /// Generates `(openBlock(true), createElementBlock(Fragment, null, renderList(<list>, (<item>) => (<expr>)), <patch flag>))`.
    /// The second `null` param can also be a branch key, e.g. `{ key: 0 }`
    pub fn generate_v_for(&mut self, v_for: &VForDirective, item_render_expr: Box<Expr>) -> Expr {
        let span = v_for.span;

        // Arrow function which renders each individual item
        let render_list_arrow = Expr::Arrow(ArrowExpr {
            span,
            ctxt: Default::default(),
            params: create_for_loop_params(&v_for.parse_result, 1),
            body: Box::new(BlockStmtOrExpr::Expr(item_render_expr)),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        });

        // `_renderList` args
        // 1. List itself, which is `v_for.parse_result.source`;
        // 2. Arrow function for each item, where argument is `v_for.iterator`
        //    and return is the passes `expr`;
        let render_list_args = vec![
            ExprOrSpread {
                spread: None,
                expr: v_for.parse_result.source.to_owned(),
            },
            ExprOrSpread {
                spread: None,
                expr: Box::new(render_list_arrow),
            },
        ];

        // `_renderList(iterable, render_list_arrow)`
        let render_list_call_expr = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                ctxt: Default::default(),
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
        let create_element_block_args = vec![
            ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(Ident {
                    span,
                    ctxt: Default::default(),
                    sym: self.get_and_add_import_ident(VueImports::Fragment),
                    optional: false,
                })),
            },
            // Note - `{ key: <number> }` is not supported here
            ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Null(Null { span }))),
            },
            ExprOrSpread {
                spread: None,
                expr: Box::new(render_list_call_expr),
            },
            ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: v_for.patch_flags.bits().into(),
                    raw: None,
                }))),
            },
        ];

        let create_element_block = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                ctxt: Default::default(),
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
    /// IN: `<div v-for="i in 3" v-memo="[]" :key="i"></div>`
    ///
    /// OUT:
    /// ```js
    /// (_openBlock(), _createElementBlock(_Fragment, null, _renderList(3, (i, __, ___, _cached) => {
    ///   const _memo = ([])
    ///   if (_cached && _cached.el && _cached.key === i && _isMemoSame(_cached, _memo)) return _cached
    ///   const _item = (_openBlock(), _createElementBlock("div", { key: i }))
    ///   _item.memo = _memo
    ///   return _item
    /// }, _cache, 4), 64 /* STABLE_FRAGMENT */))
    /// ```
    ///
    /// The second `null` param can also be a branch key, e.g. `{ key: 0 }`.
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
            expr: v_for.parse_result.source.to_owned(),
        };

        // 1.2. `_renderList` second argument - the memoized arrow function
        let render_list_arrow = ExprOrSpread {
            spread: None,
            expr: self.generate_memoized_render_arrow(
                create_for_loop_params(&v_for.parse_result, 3),
                item_render_expr,
                // Memo key is unsupported here
                ForCodegenNodeMemo {
                    expr: memo_expr,
                    key: None,
                },
            ),
        };

        // 1.3. `_renderList` third argument - `_cache`
        let render_list_cache = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(fervid_atom!("_cache").into_ident())),
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
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                ctxt: Default::default(),
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
                ctxt: Default::default(),
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
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                ctxt: Default::default(),
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
    /// IN: `<div v-for="i in 3" v-memo="[]" :key="i"></div>`
    ///
    /// OUT:
    /// ```js
    /// (i, __, ___, _cached) => {
    ///   const _memo = ([])
    ///   if (_cached && _cached.el && _cached.key === i && _isMemoSame(_cached, _memo)) return _cached
    ///   const _item = (_openBlock(), _createElementBlock("div", { key: i }))
    ///   _item.memo = _memo
    ///   return _item
    /// }
    /// ```
    fn generate_memoized_render_arrow(
        &mut self,
        mut render_list_params: Vec<Pat>,
        item_render_expr: Box<Expr>,
        memo: ForCodegenNodeMemo,
    ) -> Box<Expr> {
        // `_cached`
        let cached_ident = fervid_atom!("_cached").into_ident();
        let cached_expr = Box::new(Expr::Ident(cached_ident.to_owned()));

        // `_memo`
        let memo_ident = fervid_atom!("_memo").into_ident();

        // Params for the function
        render_list_params.push(Pat::Ident(BindingIdent {
            id: cached_ident.to_owned(),
            type_ann: None,
        }));

        // `const _memo = (memo_expr)`
        let const_memo = Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            ctxt: Default::default(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: memo_ident.to_owned(),
                    type_ann: None,
                }),
                init: Some(maybe_parenthesize(memo.expr)),
                definite: false,
            }],
        })));

        // `_cached.el`
        let cached_el = Box::new(Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: cached_expr.to_owned(),
            prop: MemberProp::Ident(IdentName {
                span: DUMMY_SP,
                sym: fervid_atom!("el"),
            }),
        }));

        // `_cached.key === memo_key`
        let cached_key_check = memo.key.map(|memo_key| {
            let cached_key = Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: cached_expr.to_owned(),
                prop: MemberProp::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: fervid_atom!("key"),
                }),
            }));

            Box::new(Expr::Bin(BinExpr {
                span: DUMMY_SP,
                op: BinaryOp::EqEqEq,
                left: cached_key,
                right: maybe_parenthesize(memo_key),
            }))
        });

        // `_isMemoSame(_cached, _memo)`
        let is_memo_same = Box::new(Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span: DUMMY_SP,
                ctxt: Default::default(),
                sym: self.get_and_add_import_ident(VueImports::IsMemoSame),
                optional: false,
            }))),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: cached_expr.to_owned(),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(memo_ident.to_owned())),
                },
            ],
            type_args: None,
        }));

        // `_cached && _cached.el && _cached.key === memo_key && _isMemoSame(_cached, _memo)`
        // `_cached.key` check is optional and depends on memo key presence
        let mut cache_cond = cached_expr.to_owned();
        for term in std::iter::once(cached_el)
            .chain(cached_key_check)
            .chain(std::iter::once(is_memo_same))
        {
            cache_cond = Box::new(Expr::Bin(BinExpr {
                span: DUMMY_SP,
                op: BinaryOp::LogicalAnd,
                left: cache_cond,
                right: term,
            }));
        }

        // `if (_cached && _cached.el && _cached.key === memo_key && _isMemoSame(_cached, _memo)) return _cached`
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
        let item_ident = fervid_atom!("_item").into_ident();

        // `const _item = item_render_expr`
        let const_item = Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            ctxt: Default::default(),
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
                left: AssignTarget::Simple(SimpleAssignTarget::Member(MemberExpr {
                    span: DUMMY_SP,
                    obj: Box::new(Expr::Ident(item_ident.to_owned())),
                    prop: MemberProp::Ident(fervid_atom!("memo").into_ident().into()),
                })),
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
            ctxt: Default::default(),
            params: render_list_params,
            body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                span: DUMMY_SP,
                ctxt: Default::default(),
                stmts: arrow_body_stmts,
            })),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        }))
    }
}

fn expr_arg(expr: Expr) -> ExprOrSpread {
    ExprOrSpread {
        spread: None,
        expr: Box::new(expr),
    }
}

fn key_object(key: Expr, span: Span) -> Expr {
    Expr::Object(ObjectLit {
        span,
        props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(
            swc_core::ecma::ast::KeyValueProp {
                key: PropName::Ident(IdentName {
                    span,
                    sym: fervid_atom!("key"),
                }),
                value: Box::new(key),
            },
        )))],
    })
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "new-pipeline")]
    use fervid_core::{
        AttributeOrBinding, BuiltinType, ElementCodegenValue, ForCodegenNodeMemo, SfcDescriptor,
        SfcTemplateBlock, VBindDirective, VNodeCall, VNodeCallTag, VueDirectives,
    };
    use fervid_core::{
        CacheMarker, ElementKind, ElementNode, ForCodegenNode, ForNode, ForParseResult, Node,
        PatchFlags, PatchHints, StartingTag,
    };

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_all_v_for_params() {
        let mut ctx = CodegenContext::default();
        let v_for = VForDirective {
            parse_result: Box::new(ForParseResult {
                source: js("items"),
                value: js("value"),
                key: Some(js("key")),
                index: Some(js("index")),
                finalized: false,
                finalized_is_dynamic: false,
            }),
            patch_flags: PatchFlags::UnkeyedFragment.into(),
            span: DUMMY_SP,
        };

        let res = ctx.generate_v_for(&v_for, js("value"));

        assert_eq!(
            crate::test_utils::to_str(res),
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(items,(value,key,index)=>value),256))"
        );

        let v_for = VForDirective {
            parse_result: Box::new(ForParseResult {
                source: js("items"),
                value: js("value"),
                key: None,
                index: Some(js("index")),
                finalized: false,
                finalized_is_dynamic: false,
            }),
            patch_flags: PatchFlags::UnkeyedFragment.into(),
            span: DUMMY_SP,
        };

        let res = ctx.generate_v_for(&v_for, js("value"));

        assert_eq!(
            crate::test_utils::to_str(res),
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(items,(value,__,index)=>value),256))"
        );
    }

    #[test]
    fn it_generates_for_nodes() {
        let dynamic_for = Node::For(for_node(
            js("items"),
            vec![element("div")],
            PatchFlags::UnkeyedFragment,
            true,
            false,
            None,
        ));
        let mut ctx = CodegenContext::default();
        let result = ctx.generate_node(&dynamic_for, true);
        assert_eq!(
            crate::test_utils::to_str(result),
            "(_openBlock(true),_createElementBlock(_Fragment,null,_renderList(items,(item)=>(_openBlock(),_createElementBlock(\"div\"))),256))"
        );

        let stable_for = Node::For(for_node(
            js("10"),
            vec![element("p")],
            PatchFlags::StableFragment,
            false,
            false,
            None,
        ));
        let mut ctx = CodegenContext::default();
        let result = ctx.generate_node(&stable_for, true);
        assert_eq!(
            crate::test_utils::to_str(result),
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(10,(item)=>_createElementVNode(\"p\")),64))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_if_v_for_keeps_branch_and_item_keys_separate() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vFor.spec.ts#L1033-L1067
        // <div v-if="ok" v-for="item in items" :key="item.id" />
        let mut node = element_with_kind(
            "div",
            ElementKind::Element,
            vec![crate::test_utils::v_bind_attribute("key", "item.id")],
        );
        let Node::Element(element) = &mut node else {
            panic!("loop input should be element");
        };
        element.starting_tag.directives = Some(Box::new(VueDirectives {
            v_if: Some(js("ok")),
            v_for: Some(VForDirective {
                parse_result: Box::new(ForParseResult {
                    source: js("items"),
                    value: js("item"),
                    key: None,
                    index: None,
                    finalized: false,
                    finalized_is_dynamic: false,
                }),
                patch_flags: Default::default(),
                span: DUMMY_SP,
            }),
            ..Default::default()
        }));

        let (_, output) = transform_and_generate_node(node);
        assert_eq!(
            output,
            "_ctx.ok?(_openBlock(true),_createElementBlock(_Fragment,{key:0},_renderList(_ctx.items,(item)=>(_openBlock(),_createElementBlock(\"div\",{key:item.id}))),128)):_createCommentVNode(\"v-if\",true)"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memoized_for_node_checks_key_only_when_present() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L58-L85
        // <div v-for="item in items" :key="item.id" v-memo="[item.rev]" />
        let mut keyed_child = element("div");
        let Node::Element(child) = &mut keyed_child else {
            panic!("expected element child");
        };
        child
            .starting_tag
            .attributes
            .push(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(fervid_atom!("key").into()),
                value: js("item.id"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            }));
        let mut keyed = for_node(
            js("items"),
            vec![keyed_child],
            PatchFlags::KeyedFragment,
            true,
            false,
            Some(js("item.id")),
        );
        let codegen = keyed
            .codegen_node
            .as_mut()
            .expect("loop needs codegen metadata");
        codegen.memo = Some(ForCodegenNodeMemo {
            expr: js("[item.rev]"),
            key: Some(js("item.id")),
        });
        let output = crate::test_utils::to_str(CodegenContext::default().generate_for_node(&keyed));
        assert!(
            output
                .contains("_cached&&_cached.el&&_cached.key===item.id&&_isMemoSame(_cached,_memo)"),
            "{output}"
        );
        assert!(output.contains("},_cache,0)"), "{output}");
        assert!(output.contains("{key:item.id}"), "{output}");

        // <div v-for="item in items" v-memo="[item.rev]" />
        let mut unkeyed = for_node(
            js("items"),
            vec![element("div")],
            PatchFlags::UnkeyedFragment,
            true,
            false,
            None,
        );
        unkeyed
            .codegen_node
            .as_mut()
            .expect("loop needs codegen metadata")
            .memo = Some(ForCodegenNodeMemo {
            expr: js("[item.rev]"),
            key: None,
        });
        let output =
            crate::test_utils::to_str(CodegenContext::default().generate_for_node(&unkeyed));
        assert!(
            output.contains("_cached&&_cached.el&&_isMemoSame(_cached,_memo)"),
            "{output}"
        );
        assert!(!output.contains("_cached.key"), "{output}");
        assert!(output.contains("},_cache,0)"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_root_and_component_uses_vnode_factory() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L11-L21
        // <div v-memo="[x]" />
        let (_, output) =
            transform_and_generate_node(memo_element("div", ElementKind::Element, "[x]"));
        assert!(output.contains("_withMemo([_ctx.x],()=>"), "{output}");
        assert!(output.contains("_createElementBlock(\"div\")"), "{output}");

        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L45-L47
        // <Comp v-memo="[x]" />
        let (_, output) =
            transform_and_generate_node(memo_element("Comp", ElementKind::Component, "[x]"));
        assert!(
            output.contains("_withMemo([_ctx.x],()=>_createVNode("),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_element_preserves_dynamic_key() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L24-L29
        // <div v-memo="[updateKey]" :key="updateKey" />
        let mut node = memo_element("div", ElementKind::Element, "[updateKey]");
        let Node::Element(element) = &mut node else {
            panic!("memo input should be element");
        };
        element
            .starting_tag
            .attributes
            .push(crate::test_utils::v_bind_attribute("key", "updateKey"));
        let (_, output) = transform_and_generate_node(node);
        assert!(output.contains("_withMemo([_ctx.updateKey]"), "{output}");
        assert!(output.contains("{key:_ctx.updateKey}"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_inside_v_if_keeps_branch_wrapper() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L49-L56
        // <div v-if="ok" v-memo="[x]" />
        let mut node = memo_element("div", ElementKind::Element, "[x]");
        let Node::Element(element) = &mut node else {
            panic!("memo input should be element");
        };
        element
            .starting_tag
            .directives
            .as_mut()
            .expect("memo directive should be present")
            .v_if = Some(js("ok"));
        let (_, output) = transform_and_generate_node(node);
        assert!(output.contains("_ctx.ok?_withMemo([_ctx.x]"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_v_if_and_v_else_keeps_both_wrappers() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L49-L56
        // <section><div v-if="ok" v-memo="[x]" /><Comp v-else v-memo="[x]" /></section>
        let mut root = element_with_kind("section", ElementKind::Element, vec![]);
        let Node::Element(section) = &mut root else {
            panic!("expected section element");
        };
        let mut if_node = memo_element("div", ElementKind::Element, "[x]");
        let Node::Element(if_element) = &mut if_node else {
            panic!("expected v-if element");
        };
        if_element
            .starting_tag
            .directives
            .as_mut()
            .expect("memo directive should be present")
            .v_if = Some(js("ok"));
        let mut else_node = memo_element("Comp", ElementKind::Component, "[x]");
        let Node::Element(else_element) = &mut else_node else {
            panic!("expected v-else component");
        };
        else_element
            .starting_tag
            .directives
            .as_mut()
            .expect("memo directive should be present")
            .v_else = Some(());
        section.children.extend([if_node, else_node]);

        let (_, output) = transform_and_generate_node(root);
        assert_eq!(
            output,
            "(_openBlock(),_createElementBlock(\"section\",null,[_ctx.ok?_withMemo([_ctx.x],()=>(_openBlock(),_createElementBlock(\"div\",{key:0})),_cache,0):_withMemo([_ctx.x],()=>(_openBlock(),_createBlock(_component_Comp,{key:1})),_cache,1)]))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn nested_memo_uses_separate_cache_entries() {
        // <div v-memo="[outer]"><span v-memo="[inner]" /></div>
        let mut root = memo_element("div", ElementKind::Element, "[outer]");
        let Node::Element(element) = &mut root else {
            panic!("expected memoized element");
        };
        element
            .children
            .push(memo_element("span", ElementKind::Element, "[inner]"));
        let (_, output) = transform_and_generate_node(root);
        assert!(output.contains("_withMemo([_ctx.inner]"), "{output}");
        assert!(output.contains("_withMemo([_ctx.outer]"), "{output}");
        assert!(output.contains("_cache,0)"), "{output}");
        assert!(output.contains("_cache,1)"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_nested_in_v_for_keeps_child_dependency_scope() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L31-L43
        // <div v-for="item in items"><div v-memo="[item.id, updateKey]" :key="get(item, updateKey)" /></div>
        let mut root = element_with_kind("div", ElementKind::Element, vec![]);
        let Node::Element(outer) = &mut root else {
            panic!("expected v-for element");
        };
        outer.starting_tag.directives = Some(Box::new(VueDirectives {
            v_for: Some(VForDirective {
                parse_result: Box::new(ForParseResult {
                    source: js("items"),
                    value: js("item"),
                    key: None,
                    index: None,
                    finalized: false,
                    finalized_is_dynamic: false,
                }),
                patch_flags: Default::default(),
                span: DUMMY_SP,
            }),
            ..Default::default()
        }));
        let mut inner = memo_element("div", ElementKind::Element, "[item.id, updateKey]");
        let Node::Element(inner_element) = &mut inner else {
            panic!("expected memoized child");
        };
        inner_element
            .starting_tag
            .attributes
            .push(crate::test_utils::v_bind_attribute(
                "key",
                "get(item, updateKey)",
            ));
        outer.children.push(inner);

        let (_, output) = transform_and_generate_node(root);
        assert!(
            output.contains("_withMemo([item.id,_ctx.updateKey]"),
            "{output}"
        );
        assert!(
            output.contains("{key:_ctx.get(item,_ctx.updateKey)}"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_v_for_compares_processed_key() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L58-L85
        // <div v-for="item in items" :key="get(item, suffix)" v-memo="[item.rev]" />
        let (_, output) = transform_and_generate_node(memo_for_element(
            "div",
            ElementKind::Element,
            "[item.rev]",
            Some("get(item, suffix)"),
        ));
        assert!(
            output.contains("_cached.key===_ctx.get(item,_ctx.suffix)"),
            "{output}"
        );
        assert!(
            output.contains("{key:_ctx.get(item,_ctx.suffix)}"),
            "{output}"
        );
        assert!(output.contains("},_cache,0)"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_v_for_parenthesizes_conditional_key() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L78-L86
        // <div v-for="item in items" :key="item.id ? item.id : fallback" v-memo="[item.rev]" />
        let (_, output) = transform_and_generate_node(memo_for_element(
            "div",
            ElementKind::Element,
            "[item.rev]",
            Some("item.id ? item.id : fallback"),
        ));
        assert!(
            output.contains("_cached.key===(item.id?item.id:_ctx.fallback)"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_template_v_for_keeps_carrier_expression() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L88-L106
        // <template v-for="item in items" :key="get(item)" v-memo="[item.rev]"><span /></template>
        let mut node = memo_for_element(
            "template",
            ElementKind::Template,
            "[item.rev]",
            Some("get(item)"),
        );
        let Node::Element(element) = &mut node else {
            panic!("memo input should be template element");
        };
        element
            .children
            .push(element_with_kind("span", ElementKind::Element, vec![]));
        let (node, output) = transform_and_generate_node(node);
        let Node::For(for_node) = node else {
            panic!("template v-for should produce ForNode");
        };
        let memo = for_node
            .codegen_node
            .as_ref()
            .and_then(|codegen| codegen.memo.as_ref())
            .expect("template carrier memo should reach loop codegen");
        assert_eq!(crate::test_utils::to_str(memo.expr.as_ref()), "[item.rev]");
        assert!(output.contains("_cached.key===_ctx.get(item)"), "{output}");
        assert!(output.contains("{key:_ctx.get(item)}"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn memo_on_template_v_for_with_multiple_children_caches_fragment() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/src/transforms/vFor.ts#L166-L180
        // <template v-for="item in items" :key="item.id" v-memo="[item.rev]">hello<span /></template>
        let mut node = memo_for_element(
            "template",
            ElementKind::Template,
            "[item.rev]",
            Some("item.id"),
        );
        let Node::Element(element) = &mut node else {
            panic!("memo input should be template element");
        };
        element.children.extend([
            Node::Text("hello".into(), DUMMY_SP),
            element_with_kind("span", ElementKind::Element, vec![]),
        ]);

        let (node, output) = transform_and_generate_node(node);
        let Node::For(for_node) = node else {
            panic!("template v-for should produce ForNode");
        };
        assert!(
            for_node
                .codegen_node
                .as_ref()
                .is_some_and(|codegen| codegen.memo.is_some()),
            "carrier memo should cover entire item fragment"
        );
        assert!(output.contains("_cached.key===item.id"), "{output}");
        assert!(
            output.contains("_createElementBlock(_Fragment,{key:item.id}"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    fn memo_element(tag: &str, kind: ElementKind, memo: &str) -> Node {
        let mut node = element_with_kind(tag, kind, vec![]);
        let Node::Element(element) = &mut node else {
            panic!("memo input should be element");
        };
        element.starting_tag.directives = Some(Box::new(VueDirectives {
            v_memo: Some(js(memo)),
            ..Default::default()
        }));
        node
    }

    #[cfg(feature = "new-pipeline")]
    fn memo_for_element(tag: &str, kind: ElementKind, memo: &str, key: Option<&str>) -> Node {
        let mut node = memo_element(tag, kind, memo);
        let Node::Element(element) = &mut node else {
            panic!("memo loop input should be element");
        };
        if let Some(key) = key {
            element
                .starting_tag
                .attributes
                .push(crate::test_utils::v_bind_attribute("key", key));
        }
        element
            .starting_tag
            .directives
            .as_mut()
            .expect("memo directive should be present")
            .v_for = Some(VForDirective {
            parse_result: Box::new(ForParseResult {
                source: js("items"),
                value: js("item"),
                key: None,
                index: None,
                finalized: false,
                finalized_is_dynamic: false,
            }),
            patch_flags: Default::default(),
            span: DUMMY_SP,
        });
        node
    }

    #[test]
    fn it_generates_cached_for_node() {
        let mut cached_for = for_node(
            js("10"),
            vec![element("p")],
            PatchFlags::StableFragment,
            false,
            false,
            None,
        );
        cached_for
            .codegen_node
            .as_mut()
            .expect("for node should have codegen metadata")
            .cache |= CacheMarker::NeedPauseTracking;
        let mut ctx = CodegenContext::default();

        let result = ctx.generate_for_node(&cached_for);

        assert_eq!(
            crate::test_utils::to_str(result),
            "_cache[0]||(_setBlockTracking(-1),(_cache[0]=(_openBlock(),_createElementBlock(_Fragment,null,_renderList(10,(item)=>_createElementVNode(\"p\")),64))).cacheIndex=0,_setBlockTracking(1),_cache[0])"
        );
    }

    #[test]
    fn it_generates_keyed_template_for_fragment() {
        let template_for = Node::For(for_node(
            js("items"),
            vec![Node::Text("hello".into(), DUMMY_SP), element("span")],
            PatchFlags::KeyedFragment,
            true,
            true,
            Some(js("item")),
        ));
        let mut ctx = CodegenContext::default();
        let result = ctx.generate_node(&template_for, true);

        assert_eq!(
            crate::test_utils::to_str(result),
            "(_openBlock(true),_createElementBlock(_Fragment,null,_renderList(items,(item)=>(_openBlock(),_createElementBlock(_Fragment,{key:item},[_createTextVNode(\"hello\"),_createElementVNode(\"span\")],64))),128))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_transformed_for_node() {
        let Node::Element(mut root) = element("div") else {
            unreachable!()
        };
        root.starting_tag.directives = Some(Box::new(fervid_core::VueDirectives {
            v_for: Some(VForDirective {
                parse_result: Box::new(ForParseResult {
                    source: js("items"),
                    value: js("item"),
                    key: None,
                    index: None,
                    finalized: false,
                    finalized_is_dynamic: false,
                }),
                patch_flags: Default::default(),
                span: DUMMY_SP,
            }),
            ..Default::default()
        }));
        let mut template = fervid_core::SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(root)],
            span: DUMMY_SP,
        };
        let descriptor = fervid_core::SfcDescriptor::default();
        let options = fervid_transform::TransformSfcOptions {
            is_prod: false,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        };
        let mut transform_ctx = fervid_transform::TransformSfcContext::new(&descriptor, &options);
        fervid_transform::template::transform_and_record_template(
            &mut template,
            &mut transform_ctx,
        );

        let mut codegen_ctx = CodegenContext::default();
        let result = codegen_ctx.generate_node(&template.roots[0], true);

        assert_eq!(
            crate::test_utils::to_str(result),
            "(_openBlock(true),_createElementBlock(_Fragment,null,_renderList(_ctx.items,(item)=>(_openBlock(),_createElementBlock(\"div\"))),256))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_need_patch_for_stable_v_for_child() {
        let Node::Element(mut root) = element("div") else {
            unreachable!()
        };
        root.starting_tag.attributes = vec![
            fervid_core::AttributeOrBinding::VBind(fervid_core::VBindDirective {
                argument: Some(fervid_atom!("key").into()),
                value: js("i"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            }),
            fervid_core::AttributeOrBinding::RegularAttribute {
                name: fervid_atom!("ref"),
                value: Some(fervid_atom!("items")),
                span: DUMMY_SP,
            },
        ];
        root.starting_tag.directives = Some(Box::new(fervid_core::VueDirectives {
            v_for: Some(VForDirective {
                parse_result: Box::new(ForParseResult {
                    source: js("3"),
                    value: js("i"),
                    key: None,
                    index: None,
                    finalized: false,
                    finalized_is_dynamic: false,
                }),
                patch_flags: Default::default(),
                span: DUMMY_SP,
            }),
            ..Default::default()
        }));
        let mut template = fervid_core::SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(root)],
            span: DUMMY_SP,
        };
        let descriptor = fervid_core::SfcDescriptor::default();
        let options = fervid_transform::TransformSfcOptions {
            is_prod: false,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        };
        let mut transform_ctx = fervid_transform::TransformSfcContext::new(&descriptor, &options);
        fervid_transform::template::transform_and_record_template(
            &mut template,
            &mut transform_ctx,
        );

        let mut codegen_ctx = CodegenContext::default();
        let result = codegen_ctx.generate_node(&template.roots[0], true);

        assert_eq!(
            crate::test_utils::to_str(result),
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(3,(i)=>_createElementVNode(\"div\",{key:i,ref_for:true,ref:\"items\"},null,512)),64))"
        );
    }

    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vFor.spec.ts#L921-L1030
    // Built-in reference: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/transformElement.spec.ts#L444-L559
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_single_plain_element_injects_typed_key() {
        // <template v-for="item in items" :key="item.id"><div /></template>
        let (node, output) = generate_keyed_template_child(element("div"));
        let vnode = keyed_template_vnode(&node);
        assert!(matches!(&vnode.tag, VNodeCallTag::Expr(_)));
        assert!(vnode.is_block);
        assert!(
            output.contains("_createElementBlock(\"div\",{key:item.id})"),
            "{output}"
        );
    }

    // Vue parser classifies only structural directives as fragment templates:
    // https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/src/parser.ts#L767-L780
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_non_structural_template_child_keeps_native_tag() {
        // <template v-for="item in items" :key="item.id"><template v-cloak><div /></template></template>
        // <template v-for="item in items" :key="item.id"><template v-pre><div /></template></template>
        for directives in [
            VueDirectives {
                v_cloak: Some(()),
                ..Default::default()
            },
            VueDirectives {
                v_pre: Some(()),
                ..Default::default()
            },
        ] {
            let mut child = element_with_kind("template", ElementKind::Element, vec![]);
            let Node::Element(element) = &mut child else {
                panic!("expected native template element")
            };
            element.starting_tag.directives = Some(Box::new(directives));
            element
                .children
                .push(element_with_kind("div", ElementKind::Element, vec![]));

            let (node, output) = generate_keyed_template_child(child);
            let vnode = keyed_template_vnode(&node);
            assert!(
                matches!(&vnode.tag, VNodeCallTag::Expr(tag) if matches!(tag.as_ref(), Expr::Lit(Lit::Str(tag)) if tag.value == "template"))
            );
            assert!(
                output.contains("_createElementBlock(\"template\",{key:item.id}"),
                "{output}"
            );
            assert!(
                !output.contains("_createElementBlock(_Fragment,{key:item.id}"),
                "{output}"
            );
        }
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_single_component_injects_typed_key() {
        // <template v-for="item in items" :key="item.id"><Comp /></template>
        let (node, output) = generate_keyed_template_child(element_with_kind(
            "Comp",
            ElementKind::Component,
            vec![],
        ));
        let vnode = keyed_template_vnode(&node);
        assert!(vnode.is_component && vnode.is_block);
        assert!(
            output.contains("_createBlock(_component_Comp,{key:item.id})"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_single_slot_uses_sixth_argument() {
        // <template v-for="item in items" :key="item.id"><slot /></template>
        let (node, output) = generate_keyed_template_child(element_with_kind(
            "slot",
            ElementKind::Builtin(BuiltinType::Slot),
            vec![],
        ));
        let Node::For(for_node) = &node else {
            panic!("expected ForNode")
        };
        let [Node::Element(child)] = for_node.children.as_slice() else {
            panic!("expected sole element")
        };
        assert!(matches!(
            child.codegen_node.as_deref().map(|codegen| &codegen.value),
            Some(ElementCodegenValue::SlotOutletCall(slot)) if slot.branch_key.is_some()
        ));
        assert!(
            output.contains("_renderSlot(_ctx.$slots,\"default\",{},undefined,undefined,item.id)"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_teleport_injects_into_typed_vnode() {
        // <template v-for="item in items" :key="item.id"><Teleport to="body" /></template>
        let (node, output) = generate_keyed_template_child(element_with_kind(
            "Teleport",
            ElementKind::Builtin(BuiltinType::Teleport),
            vec![AttributeOrBinding::RegularAttribute {
                name: "to".into(),
                value: Some("body".into()),
                span: DUMMY_SP,
            }],
        ));
        let vnode = keyed_template_vnode(&node);
        assert!(matches!(
            vnode.tag,
            VNodeCallTag::Builtin(BuiltinType::Teleport)
        ));
        assert!(vnode.is_component && vnode.is_block);
        assert!(
            output.contains("_createBlock(_Teleport,{key:item.id,to:\"body\"})"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_dynamic_component_resolves_is() {
        // <template v-for="item in items" :key="item.id"><component :is="which" /></template>
        let (node, output) = generate_keyed_template_child(element_with_kind(
            "component",
            ElementKind::Builtin(BuiltinType::Component),
            vec![AttributeOrBinding::VBind(VBindDirective {
                argument: Some("is".into()),
                value: js("which"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            })],
        ));
        let vnode = keyed_template_vnode(&node);
        assert!(
            matches!(&vnode.tag, VNodeCallTag::CallExpression(call) if call.callee == VueImports::ResolveDynamicComponent)
        );
        assert!(vnode.is_component && vnode.is_block);
        assert!(
            output.contains("_createBlock(_resolveDynamicComponent(_ctx.which),{key:item.id})"),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_other_builtins_keeps_typed_key() {
        // <template v-for="item in items" :key="item.id"><Suspense /></template>
        // Likewise for KeepAlive, BaseTransition, Transition, TransitionGroup
        for (tag, kind, helper) in [
            ("Suspense", BuiltinType::Suspense, "_Suspense"),
            ("KeepAlive", BuiltinType::KeepAlive, "_KeepAlive"),
            (
                "BaseTransition",
                BuiltinType::BaseTransition,
                "_BaseTransition",
            ),
            ("Transition", BuiltinType::Transition, "_Transition"),
            (
                "TransitionGroup",
                BuiltinType::TransitionGroup,
                "_TransitionGroup",
            ),
        ] {
            let (node, output) = generate_keyed_template_child(element_with_kind(
                tag,
                ElementKind::Builtin(kind),
                vec![],
            ));
            let vnode = keyed_template_vnode(&node);
            assert!(matches!(vnode.tag, VNodeCallTag::Builtin(_)), "{tag}");
            assert!(vnode.is_component && vnode.is_block, "{tag}");
            assert!(
                output.contains(&format!("_createBlock({helper},{{key:item.id}}")),
                "{tag}: {output}"
            );
        }
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_teleport_children_preserves_array() {
        // <template v-for="item in items" :key="item.id"><Teleport to="body"><div /></Teleport></template>
        let mut teleport = element_with_kind(
            "Teleport",
            ElementKind::Builtin(BuiltinType::Teleport),
            vec![AttributeOrBinding::RegularAttribute {
                name: "to".into(),
                value: Some("body".into()),
                span: DUMMY_SP,
            }],
        );
        let Node::Element(teleport_element) = &mut teleport else {
            panic!("expected Teleport")
        };
        teleport_element.children.push(element("div"));
        let (node, output) = generate_keyed_template_child(teleport);
        let vnode = keyed_template_vnode(&node);
        assert!(matches!(
            vnode.tag,
            VNodeCallTag::Builtin(BuiltinType::Teleport)
        ));
        assert!(
            output.contains("_createBlock(_Teleport,{key:item.id,to:\"body\"},["),
            "{output}"
        );
        assert!(output.contains("_createElementVNode(\"div\")"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_keep_alive_preserves_raw_child_and_patch_flag() {
        // <template v-for="item in items" :key="item.id"><KeepAlive><Comp /></KeepAlive></template>
        let mut keep_alive = element_with_kind(
            "KeepAlive",
            ElementKind::Builtin(BuiltinType::KeepAlive),
            vec![],
        );
        let Node::Element(keep_alive_element) = &mut keep_alive else {
            panic!("expected KeepAlive")
        };
        keep_alive_element
            .children
            .push(element_with_kind("Comp", ElementKind::Component, vec![]));
        let (node, output) = generate_keyed_template_child(keep_alive);
        let vnode = keyed_template_vnode(&node);
        assert!(vnode.patch_hints.flags.contains(PatchFlags::DynamicSlots));
        assert!(
            output.contains("_createBlock(_KeepAlive,{key:item.id},["),
            "{output}"
        );
        assert!(output.contains("_createVNode(_component_Comp)"), "{output}");
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_slot_building_builtin_preserves_default_slot() {
        // <template v-for="item in items" :key="item.id"><Suspense><div /></Suspense></template>
        // Likewise for BaseTransition, Transition, TransitionGroup
        // TODO: Vue emits stable slots (_: 1) without DYNAMIC_SLOTS for this case;
        // Fervid emits dynamic slots (_: 2) with patch flag 1024, potentially forcing extra updates
        for (tag, kind, helper) in [
            ("Suspense", BuiltinType::Suspense, "_Suspense"),
            (
                "BaseTransition",
                BuiltinType::BaseTransition,
                "_BaseTransition",
            ),
            ("Transition", BuiltinType::Transition, "_Transition"),
            (
                "TransitionGroup",
                BuiltinType::TransitionGroup,
                "_TransitionGroup",
            ),
        ] {
            let mut builtin = element_with_kind(tag, ElementKind::Builtin(kind), vec![]);
            let Node::Element(element) = &mut builtin else {
                panic!("expected built-in")
            };
            element
                .children
                .push(element_with_kind("div", ElementKind::Element, vec![]));
            let (node, output) = generate_keyed_template_child(builtin);
            let vnode = keyed_template_vnode(&node);
            assert!(
                matches!(vnode.children, Some(fervid_core::VNodeChildren::Slots(_))),
                "{tag}"
            );
            assert!(
                output.contains(&format!(
                    "_createBlock({helper},{{key:item.id}},{{\"default\":"
                )),
                "{tag}: {output}"
            );
        }
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn keyed_template_with_multiple_children_keeps_fragment_key() {
        // <template v-for="item in items" :key="item.id">hello<span /></template>
        let (node, output) = generate_keyed_template_children(vec![
            Node::Text("hello".into(), DUMMY_SP),
            element("span"),
        ]);
        let Node::For(for_node) = node else {
            panic!("expected ForNode")
        };
        assert!(matches!(
            for_node
                .codegen_node
                .as_ref()
                .and_then(|codegen| codegen.key.as_deref()),
            Some(Expr::Member(_)),
        ));
        assert!(
            output.contains("_createElementBlock(_Fragment,{key:item.id},["),
            "{output}"
        );
    }

    #[cfg(feature = "new-pipeline")]
    fn generate_keyed_template_child(child: Node) -> (Node, String) {
        generate_keyed_template_children(vec![child])
    }

    #[cfg(feature = "new-pipeline")]
    fn generate_keyed_template_children(children: Vec<Node>) -> (Node, String) {
        let root = element_with_kind(
            "template",
            ElementKind::Template,
            vec![AttributeOrBinding::VBind(VBindDirective {
                argument: Some("key".into()),
                value: js("item.id"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            })],
        );
        let Node::Element(mut root) = root else {
            panic!("expected template element")
        };
        root.children = children;
        root.starting_tag.directives = Some(Box::new(VueDirectives {
            v_for: Some(VForDirective {
                parse_result: Box::new(ForParseResult {
                    source: js("items"),
                    value: js("item"),
                    key: None,
                    index: None,
                    finalized: false,
                    finalized_is_dynamic: false,
                }),
                patch_flags: Default::default(),
                span: DUMMY_SP,
            }),
            ..Default::default()
        }));
        transform_and_generate_node(Node::Element(root))
    }

    #[cfg(feature = "new-pipeline")]
    fn transform_and_generate_node(root: Node) -> (Node, String) {
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![root],
            span: DUMMY_SP,
        };
        let descriptor = SfcDescriptor::default();
        let options = fervid_transform::TransformSfcOptions {
            is_prod: false,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        };
        let mut transform_ctx = fervid_transform::TransformSfcContext::new(&descriptor, &options);
        fervid_transform::template::transform_and_record_template(
            &mut template,
            &mut transform_ctx,
        );
        assert!(
            transform_ctx.errors.is_empty(),
            "{:#?}",
            transform_ctx.errors
        );

        let root = template
            .roots
            .pop()
            .expect("keyed template must have a root");
        let mut codegen_ctx = CodegenContext::default();
        let output = crate::test_utils::to_str(codegen_ctx.generate_node(&root, true));
        (root, output)
    }

    #[cfg(feature = "new-pipeline")]
    fn keyed_template_vnode(node: &Node) -> &VNodeCall {
        let Node::For(for_node) = node else {
            panic!("expected ForNode")
        };
        assert!(
            for_node
                .codegen_node
                .as_ref()
                .is_some_and(|codegen| codegen.key.is_none()),
            "single-child key must move out of ForCodegenNode"
        );
        let [Node::Element(child)] = for_node.children.as_slice() else {
            panic!("expected sole element")
        };
        let Some(ElementCodegenValue::VNodeCall(vnode)) =
            child.codegen_node.as_deref().map(|codegen| &codegen.value)
        else {
            panic!("expected typed child VNodeCall")
        };
        let Some(fervid_core::PropsExpression::ObjectExpression(props)) = vnode.props.as_ref()
        else {
            panic!("keyed child must have object props")
        };
        assert!(props.properties.iter().any(|prop| matches!(&prop.key, fervid_core::ExpressionPropNameNode::SimpleExpression(key) if key.ast.sym == "key")), "key missing from typed child props");
        vnode
    }

    #[cfg(feature = "new-pipeline")]
    fn element_with_kind(
        tag: &str,
        kind: ElementKind,
        attributes: Vec<AttributeOrBinding>,
    ) -> Node {
        Node::Element(ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: tag.into(),
                attributes,
                directives: None,
            },
            vec![],
            kind,
        ))
    }

    #[test]
    fn it_generates_v_for_memoized() {
        let mut ctx = CodegenContext::default();

        // `<div v-for="item in 3" v-memo="[msg]"></div>`
        let v_for = VForDirective {
            parse_result: Box::new(ForParseResult {
                source: js("3"),
                value: js("item"),
                key: None,
                index: None,
                finalized: false,
                finalized_is_dynamic: false,
            }),
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
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(3,(item,__,___,_cached)=>{const _memo=[msg.value];if(_cached&&_cached.el&&_isMemoSame(_cached,_memo))return _cached;const _item=_createElementVNode(\"div\");_item.memo=_memo;return _item;},_cache,0),64))"
        );
    }

    fn for_node(
        source: Box<Expr>,
        children: Vec<Node>,
        patch_flag: PatchFlags,
        disable_tracking: bool,
        is_template: bool,
        key: Option<Box<Expr>>,
    ) -> ForNode {
        ForNode {
            parse_result: Box::new(ForParseResult {
                source,
                value: js("item"),
                key: None,
                index: None,
                finalized: true,
                finalized_is_dynamic: true,
            }),
            children,
            template_scope: 0,
            codegen_node: Some(Box::new(ForCodegenNode {
                patch_flags: patch_flag.into(),
                disable_tracking,
                is_template,
                key,
                cache: Default::default(),
                memo: None,
                branch_key: None,
            })),
            span: DUMMY_SP,
        }
    }

    fn element(tag_name: &str) -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: tag_name.into(),
                attributes: vec![],
                directives: None,
            },
            children: vec![],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: PatchHints::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })
    }
}
