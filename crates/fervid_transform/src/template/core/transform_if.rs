use fervid_core::{
    Conditional, ConditionalNodeSequence, ConstantTypes, ElementCodegenNode, ElementCodegenValue,
    ElementKind, ElementNode, ExpressionNode, ExpressionPropNameNode, Node, PatchFlags,
    PatchFlagsSet, PatchHints, Property, SimpleExpressionNode, VNodeCall, VNodeCallTag,
    VNodeChildren, create_simple_expression_propname, fervid_atom,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, Lit, Number},
};

use crate::{
    TransformSfcContext,
    template::{
        expr_transform::BindingsHelperTransform,
        utils::{has_unnormalized_prop, inject_prop},
    },
    utils::convert_to_block,
};

pub fn transform_if(ctx: &mut TransformSfcContext, children: &mut Vec<Node>, scope_to_use: u32) {
    // Merge multiple v-if/else-if/else nodes into a ConditionalNodeSequence
    if !children.is_empty() {
        let mut seq: Option<ConditionalNodeSequence> = None;
        let mut new_children = Vec::with_capacity(children.len());

        // Auto-incremented conditional node indices
        let mut next_key_base = 0;

        /// Finishes the sequence. Pass `child` to also push the current child
        macro_rules! finish_seq {
            () => {
                if let Some(seq) = seq.take() {
                    next_key_base +=
                        1 + seq.else_if_nodes.len() + (seq.else_node.is_some() as usize);
                    new_children.push(Node::ConditionalSeq(seq))
                }
            };
            ($child: expr) => {
                finish_seq!();
                new_children.push($child);
            };
        }

        // To move out of &ElementNode to ElementNode and avoid "partially moved variable" error
        macro_rules! deref_element {
            ($child: ident) => {{
                let Node::Element(child_element) = $child else {
                    unreachable!()
                };

                Node::Element(optimize_v_if_plus_v_for(child_element))
            }};
        }

        for mut child in children.drain(..) {
            // Only process `ElementNode`s.
            // Otherwise, when we have an `if` node, ignore `Comment`s and finish sequence.
            let Node::Element(child_element) = &mut child else {
                if let (Node::Comment(_, _), Some(_)) = (&child, seq.as_ref()) {
                    continue;
                } else {
                    finish_seq!(child);
                    continue;
                }
            };

            // `<template v-slot>` structural directives belong to build_slots
            if is_template_slot_carrier(child_element) {
                if let Some(directives) = child_element.starting_tag.directives.as_mut() {
                    if let Some(ref mut v_if_cond) = directives.v_if {
                        ctx.bindings_helper.transform_expr(v_if_cond, scope_to_use);
                    }
                    if let Some(ref mut v_else_if_cond) = directives.v_else_if {
                        ctx.bindings_helper
                            .transform_expr(v_else_if_cond, scope_to_use);
                    }
                }

                finish_seq!(child);
                continue;
            }

            let Some(ref mut directives) = child_element.starting_tag.directives else {
                finish_seq!(child);
                continue;
            };

            // Check if we have a `v-if`.
            // The already existing sequence should end, and the new sequence should start.
            if let Some(v_if) = directives.v_if.take() {
                finish_seq!();
                let span = child_element.span;
                seq = Some(ConditionalNodeSequence {
                    if_node: Box::new(Conditional {
                        condition: *v_if,
                        node: deref_element!(child),
                    }),
                    else_if_nodes: vec![],
                    else_node: None,
                    span,
                    key_base: next_key_base,
                });
                continue;
            }

            // Check for `v-else-if`
            if let Some(v_else_if) = directives.v_else_if.take() {
                let Some(ref mut seq) = seq else {
                    // This must be a warning, v-else-if without v-if
                    finish_seq!(child);
                    continue;
                };

                seq.span.hi = child_element.span.hi;
                seq.else_if_nodes.push(Conditional {
                    condition: *v_else_if,
                    node: deref_element!(child),
                });
                continue;
            }

            // Check for `v-else`
            if directives.v_else.is_some() {
                let Some(ref mut cond_seq) = seq else {
                    // This must be a warning, v-else without v-if
                    finish_seq!(child);
                    continue;
                };

                cond_seq.span.hi = child_element.span.hi;
                cond_seq.else_node = Some(Box::new(deref_element!(child)));

                // `else` node always finishes the sequence
                finish_seq!();
                continue;
            }

            // No directives, just push the child
            finish_seq!(child);
        }

        // finish_seq! hand-written to not auto-increment next_key_base
        if let Some(seq) = seq.take() {
            new_children.push(Node::ConditionalSeq(seq))
        }

        *children = new_children;
    }
}

pub fn post_transform_if(ctx: &mut TransformSfcContext, node: &mut Node) {
    let Node::ConditionalSeq(seq) = node else {
        return;
    };

    let mut key = seq.key_base;
    inject_branch_key(ctx, &mut seq.if_node.node, key);

    for else_if in seq.else_if_nodes.iter_mut() {
        key += 1;
        inject_branch_key(ctx, &mut else_if.node, key);
    }

    if let Some(ref mut else_node) = seq.else_node {
        key += 1;
        inject_branch_key(ctx, else_node, key);
    }
}

// Optimize combined usage of conditional directives and `v-for`
// https://github.com/vuejs/core/blob/438a74aad840183286fbdb488178510f37218a73/packages/compiler-core/src/transforms/vIf.ts#L260
fn optimize_v_if_plus_v_for(mut parent: ElementNode) -> ElementNode {
    // Check that work is needed
    // This must be a `<template>` element with exactly one Element child
    if parent.children.len() != 1 || parent.starting_tag.tag_name != "template" {
        return parent;
    }

    let Some(Node::Element(child)) = parent.children.first_mut() else {
        return parent;
    };

    // There must be at most one `v-for` for both parent and child
    let parent_has_v_for = parent
        .starting_tag
        .directives
        .as_ref()
        .is_some_and(|d| d.v_for.is_some());
    let child_has_v_for = child
        .starting_tag
        .directives
        .as_ref()
        .is_some_and(|d| d.v_for.is_some());
    if parent_has_v_for && child_has_v_for {
        return parent;
    }

    // Take parent's `v-for` and give it to the child
    if parent_has_v_for {
        let Some(mut parent_directives) = parent.starting_tag.directives.take() else {
            unreachable!()
        };

        let child_directives = child
            .starting_tag
            .directives
            .get_or_insert_with(Default::default);
        child_directives.v_for = parent_directives.v_for.take();
    }

    // Take the child and return it instead
    let Some(Node::Element(child)) = parent.children.pop() else {
        unreachable!()
    };

    child
}

fn inject_branch_key(ctx: &mut TransformSfcContext, node: &mut Node, key: usize) {
    match node {
        Node::Element(element_node) => {
            let Some(codegen_node) = get_or_create_codegen_node(element_node) else {
                // None is returning when creating is not appropriate (e.g. non-template node).
                debug_assert!(
                    false,
                    "codegen_node could not be created for {:?}",
                    element_node.tag_type
                );
                return;
            };

            match codegen_node.value {
                ElementCodegenValue::VNodeCall(ref mut vnode_call) => {
                    convert_to_block(ctx, vnode_call);
                    inject_prop(
                        &mut vnode_call.props,
                        create_key_prop(key),
                        &mut ctx.bindings_helper,
                    )
                }
                ElementCodegenValue::WithMemoCall(ref mut with_memo_call) => {
                    convert_to_block(ctx, &mut with_memo_call.vnode_call);
                    inject_prop(
                        &mut with_memo_call.vnode_call.props,
                        create_key_prop(key),
                        &mut ctx.bindings_helper,
                    )
                }
                ElementCodegenValue::SlotOutletCall(ref mut slot_outlet_call) => {
                    // Avoid overwriting user `key`
                    if !has_unnormalized_prop(slot_outlet_call.props.as_ref(), "key") {
                        slot_outlet_call.branch_key = Some(Box::new(key.into()));
                    }
                }
            }
        }
        Node::For(for_node) => {
            let Some(codegen_node) = for_node.codegen_node.as_mut() else {
                debug_assert!(false, "For codegen_node was empty");
                return;
            };

            codegen_node.branch_key = Some(key);
        }

        // These branches should not be possible:
        // Node::Text
        // Node::Interpolation
        // Node::Comment
        // Node::ConditionalSeq
        _ => {
            debug_assert!(false, "Unexpected Node type inside Conditional: {:?}", node)
        }
    }
}

fn get_or_create_codegen_node(element_node: &mut ElementNode) -> Option<&mut ElementCodegenNode> {
    if let Some(ref mut value) = element_node.codegen_node {
        return Some(value);
    }

    if !matches!(element_node.tag_type, ElementKind::Template) {
        return None;
    }

    element_node.codegen_node = Some(Box::new(ElementCodegenNode {
        value: ElementCodegenValue::VNodeCall(Box::new(VNodeCall {
            tag: VNodeCallTag::Fragment,
            props: None,
            children: Some(VNodeChildren::UseElementChildren),
            patch_hints: PatchHints {
                flags: PatchFlagsSet::from(PatchFlags::StableFragment),
                props: vec![],
                should_use_block: false,
            },
            directives: None,
            needs_patch: false,
            is_block_required: false,
            is_block: true,
            disable_tracking: false,
            is_component: false,
        })),
        cache: Default::default(),
    }));
    element_node.codegen_node.as_deref_mut()
}

/// Creates `key: 123` object Property
fn create_key_prop(key: usize) -> Property {
    Property {
        key: ExpressionPropNameNode::SimpleExpression(create_simple_expression_propname(
            fervid_atom!("key"),
            true,
            DUMMY_SP,
        )),
        value: fervid_core::JsChildNode::ExpressionNode(Box::new(
            ExpressionNode::SimpleExpression(SimpleExpressionNode {
                ast: Box::new(Expr::Lit(Lit::Num(Number {
                    span: DUMMY_SP,
                    value: key as f64,
                    raw: None,
                }))),
                is_static: false,
                const_type: ConstantTypes::CanCache,
                is_handler_key: false,
            }),
        )),
        span: DUMMY_SP,
    }
}

fn is_template_slot_carrier(node: &ElementNode) -> bool {
    matches!(node.tag_type, ElementKind::Template)
        && node
            .starting_tag
            .directives
            .as_ref()
            .is_some_and(|directives| directives.v_slot.is_some())
}

#[cfg(test)]
mod tests {
    use fervid_core::VueDirectives;

    use crate::test_utils::{element_from_tag, js};

    use super::*;

    #[test]
    fn sibling_v_if_sequences_have_distinct_key_bases() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L637-L660
        // <div v-if="ok" /><p v-if="orNot" />
        let mut children = vec![if_branch("div", "ok"), if_branch("p", "orNot")];

        transform_if(&mut TransformSfcContext::anonymous(), &mut children, 0);

        let [Node::ConditionalSeq(first), Node::ConditionalSeq(second)] = children.as_slice()
        else {
            panic!("independent v-if siblings should become separate sequences");
        };
        assert_eq!(first.key_base, 0);
        assert_eq!(second.key_base, 1);
    }

    #[test]
    fn key_base_counts_preceding_branches_not_plain_siblings() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L662-L700
        // <div v-if="ok" /><p v-else /><hr /><div v-if="another" /><p v-else-if="orNot" /><p v-else />
        let mut children = vec![
            if_branch("div", "ok"),
            branch(
                "p",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
            Node::Element(element_from_tag("hr")),
            if_branch("div", "another"),
            branch(
                "p",
                VueDirectives {
                    v_else_if: Some(js("orNot")),
                    ..Default::default()
                },
            ),
            branch(
                "p",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
        ];

        transform_if(&mut TransformSfcContext::anonymous(), &mut children, 0);

        let [
            Node::ConditionalSeq(first),
            Node::Element(_),
            Node::ConditionalSeq(second),
        ] = children.as_slice()
        else {
            panic!("plain sibling should separate conditional sequences");
        };
        assert_eq!(first.key_base, 0);
        assert!(first.else_node.is_some());
        assert_eq!(second.key_base, 2);
        assert_eq!(second.else_if_nodes.len(), 1);
        assert!(second.else_node.is_some());
    }

    fn branch(tag: &str, directives: VueDirectives) -> Node {
        let mut element = element_from_tag(tag);
        element.starting_tag.directives = Some(Box::new(directives));
        Node::Element(element)
    }

    fn if_branch(tag: &str, condition: &str) -> Node {
        branch(
            tag,
            VueDirectives {
                v_if: Some(js(condition)),
                ..Default::default()
            },
        )
    }
}
