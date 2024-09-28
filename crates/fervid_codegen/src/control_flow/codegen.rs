use fervid_core::{ElementKind, ElementNode, IntoIdent, Node, VueImports};
use smallvec::SmallVec;
use swc_core::{
    common::{BytePos, Span},
    ecma::ast::{
        BinExpr, BinaryOp, CallExpr, Callee, Expr, ExprOrSpread, Lit, Number, ParenExpr, SeqExpr,
    },
};

use crate::context::CodegenContext;

type TextNodesConcatenationVec = SmallVec<[Expr; 3]>;

impl CodegenContext {
    pub fn generate_node(&mut self, node: &Node, wrap_in_block: bool) -> Expr {
        match node {
            Node::Text(contents, span) => self.generate_text_node(contents, span.to_owned()),

            Node::Interpolation(interpolation) => self.generate_interpolation(interpolation),

            Node::Element(element_node) => {
                self.generate_element_or_component(element_node, wrap_in_block)
            }

            Node::Comment(comment, span) => self.generate_comment_vnode(comment, span.to_owned()),

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
        let has_v_once = element_node
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
        let mut result = match element_node.kind {
            ElementKind::Builtin(builtin_type) => self.generate_builtin(element_node, builtin_type),
            ElementKind::Element => self.generate_element_vnode(element_node, wrap_in_block),
            ElementKind::Component => self.generate_component_vnode(element_node, wrap_in_block),
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
            result = self.generate_v_once(Box::new(result));

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

        while let Some(node) = iter.next() {
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
        Expr::Paren(ParenExpr {
            span,
            expr: Box::new(Expr::Seq(SeqExpr {
                span,
                exprs: vec![
                    // openBlock()
                    Box::new(Expr::Call(CallExpr {
                        span,
                        ctxt: Default::default(),
                        callee: Callee::Expr(Box::new(Expr::Ident(
                            self.get_and_add_import_ident(VueImports::OpenBlock)
                                .into_ident_spanned(span),
                        ))),
                        args: Vec::new(),
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
