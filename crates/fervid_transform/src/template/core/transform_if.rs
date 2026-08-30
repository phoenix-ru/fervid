use fervid_core::{Conditional, ConditionalNodeSequence, ElementKind, ElementNode, Node};

use crate::{TransformSfcContext, template::expr_transform::BindingsHelperTransform};

pub fn transform_if(ctx: &mut TransformSfcContext, children: &mut Vec<Node>, scope_to_use: u32) {
    // Merge multiple v-if/else-if/else nodes into a ConditionalNodeSequence
    if !children.is_empty() {
        let mut seq: Option<ConditionalNodeSequence> = None;
        let mut new_children = Vec::with_capacity(children.len());

        /// Finishes the sequence. Pass `child` to also push the current child
        macro_rules! finish_seq {
            () => {
                if let Some(seq) = seq.take() {
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

        finish_seq!();

        *children = new_children;
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

fn is_template_slot_carrier(node: &ElementNode) -> bool {
    matches!(node.tag_type, ElementKind::Template)
        && node
            .starting_tag
            .directives
            .as_ref()
            .is_some_and(|directives| directives.v_slot.is_some())
}
