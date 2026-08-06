use fervid_core::{
    ForNode, ForParseResult, IntoIdent, Node, PatchFlags, VForDirective, VueImports, fervid_atom,
};
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

fn create_for_loop_params(result: &ForParseResult, minimum_len: usize) -> Vec<Pat> {
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

        let mut item_render_expr = if let [Node::Element(_)] = for_node.children.as_slice() {
            self.generate_node(&for_node.children[0], !is_stable)
        } else {
            self.generate_for_fragment(&for_node.children, codegen_node.key.as_deref(), span)
        };

        if codegen_node.is_template
            && matches!(for_node.children.as_slice(), [Node::Element(_)])
            && let Some(key) = codegen_node.key.as_deref()
        {
            inject_key(self, &mut item_render_expr, key.clone(), span);
        }

        let render_list_arrow = Expr::Arrow(ArrowExpr {
            span,
            ctxt: Default::default(),
            params: create_for_loop_params(&for_node.parse_result, 1),
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(item_render_expr))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        });
        let render_list = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::RenderList)
                    .into_ident_spanned(span),
            ))),
            args: vec![
                expr_arg(*for_node.parse_result.source.clone()),
                expr_arg(render_list_arrow),
            ],
            type_args: None,
        });
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
                expr_arg(Expr::Lit(Lit::Null(Null { span }))),
                expr_arg(render_list),
                expr_arg(Expr::Lit(Lit::Num(Number {
                    span,
                    value: codegen_node.patch_flags.bits().into(),
                    raw: None,
                }))),
            ],
            type_args: None,
        });

        self.wrap_in_open_block_with_tracking(fragment, span, codegen_node.disable_tracking)
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

    /// Generates `(openBlock(true), createElementBlock(Fragment, null, renderList(<list>, (<item>) => (<expr>)), <patch flag>))`
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
            expr: v_for.parse_result.source.to_owned(),
        };

        // 1.2. `_renderList` second argument - the memoized arrow function
        let render_list_arrow = ExprOrSpread {
            spread: None,
            expr: self.generate_memoized_render_arrow(
                create_for_loop_params(&v_for.parse_result, 3),
                item_render_expr,
                memo_expr,
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
        mut render_list_params: Vec<Pat>,
        item_render_expr: Box<Expr>,
        memo_expr: Box<Expr>,
    ) -> Box<Expr> {
        // `_cached`
        let cached_ident = fervid_atom!("_cached").into_ident();

        // `_memo`
        let memo_ident = fervid_atom!("_memo").into_ident();

        // Params for the function
        render_list_params.push(Pat::Ident(BindingIdent {
            id: cached_ident.to_owned(),
            type_ann: None,
        }));

        // `const _memo = ([])`
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
                init: Some(memo_expr),
                definite: false,
            }],
        })));

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

// TODO: Move to v_for codegen transform and use better AST instead of guessing
fn inject_key(ctx: &mut CodegenContext, expr: &mut Expr, key: Expr, span: Span) {
    match expr {
        Expr::Paren(paren) => inject_key(ctx, &mut paren.expr, key, span),
        Expr::Seq(sequence) => {
            if let Some(last) = sequence.exprs.last_mut() {
                inject_key(ctx, last, key, span);
            }
        }
        Expr::Call(call) => {
            if let Some(first) = call.args.first_mut()
                && matches!(first.expr.as_ref(), Expr::Paren(_) | Expr::Seq(_))
            {
                inject_key(ctx, &mut first.expr, key, span);
                return;
            }

            let key_object = key_object(key, span);
            if call.args.len() < 2 {
                call.args.push(expr_arg(key_object));
                return;
            }

            let props = &mut call.args[1].expr;
            match props.as_mut() {
                Expr::Lit(Lit::Null(_)) => **props = key_object,
                Expr::Object(object) => object.props.extend(match key_object {
                    Expr::Object(key_object) => key_object.props,
                    _ => unreachable!(),
                }),
                _ => {
                    let existing =
                        std::mem::replace(props, Box::new(Expr::Lit(Lit::Null(Null { span }))));
                    **props = Expr::Call(CallExpr {
                        span,
                        ctxt: Default::default(),
                        callee: Callee::Expr(Box::new(Expr::Ident(
                            ctx.get_and_add_import_ident(VueImports::MergeProps)
                                .into_ident_spanned(span),
                        ))),
                        args: vec![expr_arg(*existing), expr_arg(key_object)],
                        type_args: None,
                    });
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        ElementKind, ElementNode, ForCodegenNode, ForNode, ForParseResult, Node, PatchFlags,
        PatchHints, StartingTag,
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
                argument: Some(fervid_core::StrOrExpr::Str(fervid_atom!("key"))),
                value: js("i"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            }),
            fervid_core::AttributeOrBinding::RegularAttribute {
                name: fervid_atom!("ref"),
                value: fervid_atom!("items"),
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
            "(_openBlock(),_createElementBlock(_Fragment,null,_renderList(3,(item,__,___,_cached)=>{const _memo=[msg.value];if(_cached&&_isMemoSame(_cached,_memo))return _cached;const _item=_createElementVNode(\"div\");_item.memo=_memo;return _item;},_cache,0),64))"
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
