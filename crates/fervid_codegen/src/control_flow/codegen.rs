use fervid_core::{
    CacheMarker, CacheMarkers, ElementKind, ElementNode, IntoIdent, Node, VueImports, fervid_atom,
};
use smallvec::SmallVec;
use swc_core::{
    common::{BytePos, DUMMY_SP, Span},
    ecma::ast::{
        ArrayLit, AssignExpr, AssignOp, AssignTarget, BinExpr, BinaryOp, Bool, CallExpr, Callee,
        ComputedPropName, Expr, ExprOrSpread, IdentName, Lit, MemberExpr, MemberProp, Number,
        ParenExpr, SeqExpr, SimpleAssignTarget,
    },
};

use crate::context::CodegenContext;

type TextNodesConcatenationVec = SmallVec<[Expr; 3]>;

impl CodegenContext {
    pub(crate) fn wrap_cache_expression(
        &mut self,
        index: u8,
        value: Expr,
        markers: CacheMarkers,
    ) -> Expr {
        // _cache[index]
        let cache_member = MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(fervid_atom!("_cache").into_ident())),
            prop: MemberProp::Computed(ComputedPropName {
                span: DUMMY_SP,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span: DUMMY_SP,
                    value: index as f64,
                    raw: None,
                }))),
            }),
        };
        let cache_expr = Expr::Member(cache_member.clone());
        // _cache[index] = value
        let cache_assign = Expr::Assign(AssignExpr {
            span: DUMMY_SP,
            op: AssignOp::Assign,
            left: AssignTarget::Simple(SimpleAssignTarget::Member(cache_member)),
            right: Box::new(value),
        });

        let right = if markers.contains(CacheMarker::NeedPauseTracking) {
            let set_block_tracking = self
                .get_and_add_import_ident(VueImports::SetBlockTracking)
                .into_ident();
            // _setBlockTracking(value) or _setBlockTracking(value, true)
            let set_tracking = |value: f64, in_v_once: bool| {
                let mut args = vec![ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value,
                        raw: None,
                    }))),
                }];
                if in_v_once {
                    args.push(ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Lit(Lit::Bool(Bool {
                            span: DUMMY_SP,
                            value: true,
                        }))),
                    });
                }
                Box::new(Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(set_block_tracking.clone()))),
                    args,
                    type_args: None,
                }))
            };
            // (_cache[index] = value).cacheIndex = index
            let assign_cache_index = Box::new(Expr::Assign(AssignExpr {
                span: DUMMY_SP,
                op: AssignOp::Assign,
                left: AssignTarget::Simple(SimpleAssignTarget::Member(MemberExpr {
                    span: DUMMY_SP,
                    obj: Box::new(Expr::Paren(ParenExpr {
                        span: DUMMY_SP,
                        expr: Box::new(cache_assign),
                    })),
                    prop: MemberProp::Ident(IdentName {
                        span: DUMMY_SP,
                        sym: fervid_atom!("cacheIndex"),
                    }),
                })),
                right: Box::new(Expr::Lit(Lit::Num(Number {
                    span: DUMMY_SP,
                    value: index as f64,
                    raw: None,
                }))),
            }));

            Box::new(Expr::Paren(ParenExpr {
                span: DUMMY_SP,
                expr: Box::new(Expr::Seq(SeqExpr {
                    span: DUMMY_SP,
                    exprs: vec![
                        set_tracking(-1.0, markers.contains(CacheMarker::InVOnce)),
                        assign_cache_index,
                        set_tracking(1.0, false),
                        Box::new(cache_expr.clone()),
                    ],
                })),
            }))
        } else {
            Box::new(Expr::Paren(ParenExpr {
                span: DUMMY_SP,
                expr: Box::new(cache_assign),
            }))
        };

        let cached = Expr::Bin(BinExpr {
            span: DUMMY_SP,
            op: BinaryOp::LogicalOr,
            left: Box::new(cache_expr),
            right,
        });

        if markers.contains(CacheMarker::NeedArraySpread) {
            // [...(_cache[index] || (_cache[index] = value))]
            Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: vec![Some(ExprOrSpread {
                    spread: Some(DUMMY_SP),
                    expr: Box::new(Expr::Paren(ParenExpr {
                        span: DUMMY_SP,
                        expr: Box::new(cached),
                    })),
                })],
            })
        } else {
            cached
        }
    }

    pub fn generate_node(&mut self, node: &Node, wrap_in_block: bool) -> Expr {
        match node {
            Node::Text(contents, span) => self.generate_text_node(contents, span.to_owned()),

            Node::Interpolation(interpolation) => self.generate_interpolation(interpolation),

            Node::Element(element_node) => {
                self.generate_element_or_component(element_node, wrap_in_block)
            }

            Node::Comment(comment, span) => self.generate_comment_vnode(comment, span.to_owned()),

            Node::For(for_node) => self.generate_for_node(for_node),

            Node::ConditionalSeq(conditional_seq) => self.generate_conditional_seq(conditional_seq),
        }
    }

    /// Generates the HTML element, component or a Vue built-in.
    pub fn generate_element_or_component(
        &mut self,
        element_node: &ElementNode,
        wrap_in_block: bool,
    ) -> Expr {
        // `v-once` logic is common for all
        #[cfg(not(feature = "new-pipeline"))]
        let has_v_once = element_node
            .starting_tag
            .directives
            .as_ref()
            .and_then(|directives| directives.v_once)
            .is_some();
        #[cfg(feature = "new-pipeline")]
        let has_v_once = element_node.codegen_node.is_none()
            && element_node
                .starting_tag
                .directives
                .as_ref()
                .and_then(|directives| directives.v_once)
                .is_some();

        // Disable caching if `v-once` is present
        let old_is_cache_disabled = self.is_cache_disabled;
        if has_v_once {
            self.is_cache_disabled = true;
        }

        // Generate the relevant render code depending on ElementKind
        let mut result = {
            #[cfg(feature = "new-pipeline")]
            if let Some(codegen_node) = element_node.codegen_node.as_deref() {
                self.generate_element_codegen_node(element_node, codegen_node, wrap_in_block)
            } else {
                match element_node.tag_type {
                    ElementKind::Builtin(builtin_type) => {
                        self.generate_builtin(element_node, builtin_type)
                    }
                    ElementKind::Element | ElementKind::Template => {
                        self.generate_element_vnode(element_node, wrap_in_block)
                    }
                    ElementKind::Component => {
                        self.generate_component_vnode(element_node, wrap_in_block)
                    }
                }
            }

            #[cfg(not(feature = "new-pipeline"))]
            match element_node.tag_type {
                ElementKind::Builtin(builtin_type) => {
                    self.generate_builtin(element_node, builtin_type)
                }
                ElementKind::Element | ElementKind::Template => {
                    self.generate_element_vnode(element_node, wrap_in_block)
                }
                ElementKind::Component => {
                    self.generate_component_vnode(element_node, wrap_in_block)
                }
            }
        };

        // Generate directives operating on render code
        if let Some(ref directives) = element_node.starting_tag.directives {
            // This block generates `v-for` and `v-memo`.
            // These are dependent on each other, therefore need to be generated like that.
            match (directives.v_for.as_ref(), directives.v_memo.as_ref()) {
                (None, None) => {}
                (None, Some(v_memo)) => {
                    result = self.generate_v_memo(v_memo.to_owned(), Box::new(result));
                }
                (Some(v_for), None) => {
                    result = self.generate_v_for(v_for, Box::new(result));
                }
                (Some(v_for), Some(v_memo)) => {
                    result =
                        self.generate_v_for_memoized(v_for, Box::new(result), v_memo.to_owned());
                }
            }
        }

        // Generate `v-once` if needed (also operates on render code)
        if has_v_once {
            result = self.generate_v_once(result);

            // Restore caching
            self.is_cache_disabled = old_is_cache_disabled;
        }

        result
    }

    /// Generates a sequence of nodes taken from an iterator.
    ///
    /// - `total_nodes` is a hint of how many nodes are in the original Vec,
    ///   it will be used when deciding whether to inline or not;
    /// - `allow_inlining` is whether all text nodes can be merged
    ///   without a surrounding `createTextVNode` call.
    ///
    /// Returns `true` if all the nodes were inlined successfully
    #[allow(unused_assignments)]
    pub fn generate_node_sequence<'n>(
        &mut self,
        iter: &mut impl Iterator<Item = &'n Node>,
        out: &mut Vec<Expr>,
        total_nodes: usize,
        allow_inlining: bool,
    ) -> bool {
        // Buffer for concatenating text nodes. Will be reused multiple times
        let mut text_nodes = TextNodesConcatenationVec::new();
        let mut text_nodes_span = [BytePos(0), BytePos(0)];
        let mut patch_flag_text = false;

        macro_rules! maybe_concatenate_text_nodes {
            () => {
                if text_nodes.len() != 0 {
                    // Ignore `createTextVNode` if allowed and all the nodes are text nodes
                    let should_inline = allow_inlining && text_nodes.len() == total_nodes;
                    let concatenation = self.concatenate_text_nodes(
                        &mut text_nodes,
                        should_inline,
                        Span {
                            lo: text_nodes_span[0],
                            hi: text_nodes_span[1],
                        },
                        patch_flag_text,
                    );
                    out.push(concatenation);

                    // Reset text nodes
                    text_nodes.clear();
                    text_nodes_span[0] = BytePos(0);
                    text_nodes_span[1] = BytePos(0);

                    // Return whether was inlined or not
                    should_inline
                } else {
                    false
                }
            };
        }

        for node in iter.by_ref() {
            let generated = self.generate_node(node, false);
            let is_text_node = matches!(node, Node::Text(_, _) | Node::Interpolation { .. });

            if let Node::Interpolation(interpolation) = node {
                patch_flag_text |= interpolation.patch_flag;
            }

            if is_text_node {
                text_nodes.push(generated);

                // Save span
                // TODO real spans
                if text_nodes_span[0].is_dummy() {
                    text_nodes_span[0] = BytePos(0);
                }
                text_nodes_span[1] = BytePos(0);
            } else {
                // Process the text nodes from before
                maybe_concatenate_text_nodes!();
                patch_flag_text = false;

                out.push(generated);
            }
        }

        // Process the remaining text nodes.
        maybe_concatenate_text_nodes!()
    }

    /// Wraps the expression in openBlock construction,
    /// e.g. `(openBlock(), expr)`
    pub fn wrap_in_open_block(&mut self, expr: Expr, span: Span) -> Expr {
        self.wrap_in_open_block_with_tracking(expr, span, false)
    }

    pub fn wrap_in_open_block_with_tracking(
        &mut self,
        expr: Expr,
        span: Span,
        disable_tracking: bool,
    ) -> Expr {
        let args = disable_tracking.then(|| ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Bool(Bool { span, value: true }))),
        });

        Expr::Paren(ParenExpr {
            span,
            expr: Box::new(Expr::Seq(SeqExpr {
                span,
                exprs: vec![
                    // openBlock() or openBlock(true)
                    Box::new(Expr::Call(CallExpr {
                        span,
                        ctxt: Default::default(),
                        callee: Callee::Expr(Box::new(Expr::Ident(
                            self.get_and_add_import_ident(VueImports::OpenBlock)
                                .into_ident_spanned(span),
                        ))),
                        args: args.into_iter().collect(),
                        type_args: None,
                    })),
                    Box::new(expr),
                ],
            })),
        })
    }

    /// Special case: `<template>` with `v-if`/`v-else-if`/`v-else`/`v-for`
    #[inline]
    pub fn should_generate_fragment(&self, element_node: &ElementNode) -> bool {
        element_node.starting_tag.tag_name.eq("template")
            && match element_node.starting_tag.directives {
                Some(ref directives) => {
                    directives.v_if.is_some()
                        || directives.v_else_if.is_some()
                        || directives.v_else.is_some()
                        || directives.v_for.is_some()
                }
                None => false,
            }
    }

    /// Produce the index for a next `cache[idx]` entry.
    /// This is useful for a `v-once` or event handlers.
    pub fn allocate_next_cache_entry(&mut self) -> u8 {
        let idx = self.next_cache_index;
        self.next_cache_index += 1;
        idx
    }

    fn concatenate_text_nodes(
        &mut self,
        text_nodes_concatenation: &mut TextNodesConcatenationVec,
        inline: bool,
        span: Span,
        patch_flag_text: bool,
    ) -> Expr {
        let concatenation: Expr = join_exprs_to_concatenation(text_nodes_concatenation, span);

        // In `inline` mode, just return concatenation as-is
        // Otherwise surround with `createTextVNode()`
        if inline {
            return concatenation;
        }

        // `concatenation`
        let mut create_text_vnode_args = Vec::with_capacity(if patch_flag_text { 2 } else { 1 });
        create_text_vnode_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(concatenation),
        });

        // Add patch flag
        // `concatenation, 1`
        if patch_flag_text {
            create_text_vnode_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: 1.0,
                    raw: None,
                }))),
            })
        }

        // createTextVNode(/* args */)
        Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::CreateTextVNode)
                    .into_ident_spanned(span),
            ))),
            args: create_text_vnode_args,
            type_args: None,
        })
    }
}

/// Concatenate multiple expressions, e.g. `expr1 + expr2 + expr3`
fn join_exprs_to_concatenation(exprs: &mut TextNodesConcatenationVec, span: Span) -> Expr {
    let mut drain = exprs.drain(..);

    let mut expr = drain.next().expect("TextNodesConcatenationVec was empty");

    for item in drain {
        expr = Expr::Bin(BinExpr {
            span,
            op: BinaryOp::Add,
            left: Box::new(expr),
            right: Box::new(item),
        })
    }

    expr
}
