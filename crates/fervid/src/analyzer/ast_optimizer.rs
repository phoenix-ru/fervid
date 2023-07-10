use fervid_core::{ConditionalNodeSequence, ElementNode, Node, SfcTemplateBlock, StartingTag};

use crate::compiler::all_html_tags;

pub fn optimize_template<'a>(template: &'a mut SfcTemplateBlock) -> &'a SfcTemplateBlock<'a> {
    // Only retain `ElementNode`s as template roots
    template
        .roots
        .retain(|root| matches!(root, Node::Element(_)));

    // Optimize conditional sequences within template root
    optimize_children(&mut template.roots, false);

    // Todo merge 1+ children into a separate `<template>` element so that Fragment gets generated

    // Optimize each root node separately
    let mut ast_optimizer = AstOptimizer;
    let ast = &mut template.roots;
    let mut iter = ast.iter_mut();
    while let Some(ref mut node) = iter.next() {
        node.visit_mut_with(&mut ast_optimizer);
    }

    template
}

/// Optimizes the children by removing whitespace in between `ElementNode`s,
///
fn optimize_children(children: &mut Vec<Node>, is_component: bool) {
    let children_len = children.len();

    // Discard children mask, limited to 128 children. 0 means to preserve the node, 1 to discard
    let mut discard_mask: u128 = 0;

    // Filter out whitespace text nodes at the beginning and end of ElementNode
    match children.first() {
        Some(Node::Text(v)) if v.trim().len() == 0 => {
            discard_mask |= 1 << 0;
        }
        _ => {}
    }
    match children.last() {
        Some(Node::Text(v)) if v.trim().len() == 0 => {
            discard_mask |= 1 << (children_len - 1);
        }
        _ => {}
    }

    // For removing the middle whitespace text nodes, we need sliding windows of three nodes
    for (index, window) in children.windows(3).enumerate() {
        match window {
            [Node::Element(_) | Node::Comment(_), Node::Text(middle), Node::Element(_) | Node::Comment(_)]
                if middle.trim().len() == 0 =>
            {
                discard_mask |= 1 << (index + 1);
            }
            _ => {}
        }
    }

    // Retain based on discard_mask. If a discard bit at `index` is set to 1, the node will be dropped
    let mut index = 0;
    children.retain(|_| {
        let should_retain = discard_mask & (1 << index) == 0;
        index += 1;
        should_retain
    });

    // For components, reorder children so that named slots come first
    if is_component && children.len() > 0 {
        children.sort_by(|a, b| {
            let a_is_from_default = is_from_default_slot(a);
            let b_is_from_default = is_from_default_slot(b);

            a_is_from_default.cmp(&b_is_from_default)
        });
    }

    // Merge multiple v-if/else-if/else nodes into a ConditionalNodeSequence
    if children.len() != 0 {
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
                let Node::Element(child_element) = $child else { unreachable!() };
                child_element
            }};
        }

        for child in children.drain(..) {
            // Only process `ElementNode`s.
            // Otherwise, when we have an `if` node, ignore `Comment`s and finish sequence.
            let Node::Element(child_element) = &child else {
                if let (Node::Comment(_), Some(_)) = (&child, seq.as_ref()) {
                    continue;
                } else {
                    finish_seq!(child);
                    continue;
                }
            };

            let Some(ref directives) = child_element.starting_tag.directives else {
                finish_seq!(child);
                continue;
            };

            // Check if we have a `v-if`.
            // The already existing sequence should end, and the new sequence should start.
            if let Some(v_if) = directives.v_if {
                finish_seq!();
                seq = Some(ConditionalNodeSequence {
                    if_node: (v_if, Box::new(deref_element!(child))),
                    else_if_nodes: vec![],
                    else_node: None,
                });
                continue;
            }

            // Check for `v-else-if`
            if let Some(v_else_if) = directives.v_else_if {
                let Some(ref mut seq) = seq else {
                    // This must be a warning, v-else-if without v-if
                    finish_seq!(child);
                    continue;
                };

                seq.else_if_nodes.push((v_else_if, deref_element!(child)));
                continue;
            }

            // Check for `v-else`
            if let Some(_) = directives.v_else {
                let Some(ref mut cond_seq) = seq else {
                    // This must be a warning, v-else without v-if
                    finish_seq!(child);
                    continue;
                };

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

struct AstOptimizer;

trait Visitor {
    fn visit_element_node(&mut self, element_node: &mut ElementNode);
}

trait VisitMut {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor);
}

trait VisitMutChildren {
    fn visit_mut_children_with(&mut self, visitor: &mut impl Visitor);
}

impl<'a> Visitor for AstOptimizer {
    fn visit_element_node(&mut self, element_node: &mut ElementNode) {
        optimize_children(
            &mut element_node.children,
            self.is_component(&element_node.starting_tag),
        );
        element_node.visit_mut_children_with(self);
    }
}

impl AstOptimizer {
    fn is_component(&self, starting_tag: &StartingTag) -> bool {
        // TODO Use is_custom_element as well
        !all_html_tags::is_html_tag(starting_tag.tag_name)
    }
}

impl VisitMut for Node<'_> {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor) {
        match self {
            Node::Element(el) => el.visit_mut_with(visitor),
            _ => {}
        }
    }
}

impl VisitMut for ElementNode<'_> {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor) {
        visitor.visit_element_node(self);
    }
}

impl VisitMutChildren for ElementNode<'_> {
    fn visit_mut_children_with(&mut self, visitor: &mut impl Visitor) {
        for child in &mut self.children {
            child.visit_mut_with(visitor)
        }
    }
}

fn is_from_default_slot(node: &Node) -> bool {
    let Node::Element(ElementNode { starting_tag, .. }) = node else {
        return true;
    };

    if starting_tag.tag_name != "template" {
        return true;
    }

    // Slot is not default if its `v-slot` has an argument which is not "" or "default"
    // `v-slot` is default
    // `v-slot:default` is default
    // `v-slot:custom` is not default
    let Some(ref directives) = starting_tag.directives else { return true; };
    let Some(ref v_slot) = directives.v_slot else { return true; };
    if v_slot.is_dynamic_slot {
        return false;
    }

    match v_slot.slot_name {
        None | Some("default") => true,
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{Node, VueDirectives};

    use super::*;

    #[test]
    fn it_folds_basic_seq() {
        // <template><div>
        //   text
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h3 v-else>else</h3>
        // </div></template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![text_node(), if_node(), else_if_node(), else_node()],
                template_scope: 0,
            })],
        };

        optimize_template(&mut sfc_template);

        // Template roots: one div
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref div) = sfc_template.roots[0] else {
            panic!("Root is not an element")
        };

        // Text and conditional seq
        assert_eq!(2, div.children.len());
        check_text_node(&div.children[0]);
        let Node::ConditionalSeq(seq) = &div.children[1] else {
            panic!("Not a conditional sequence")
        };

        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);

        // <h2 v-else-if="foo">else-if</h3>
        assert_eq!(1, seq.else_if_nodes.len());
        check_else_if_node(&seq.else_if_nodes[0]);

        // <h3 v-else>else</h3>
        check_else_node(seq.else_node.as_ref());
    }

    #[test]
    fn it_folds_roots() {
        // <template>
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h3 v-else>else</h3>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![if_node(), else_if_node(), else_node()],
        };

        optimize_template(&mut sfc_template);

        // Template roots: one conditional sequence
        assert_eq!(1, sfc_template.roots.len());
        let Node::ConditionalSeq(ref seq) = sfc_template.roots[0] else {
            panic!("Root is not a conditional sequence")
        };

        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);

        // <h2 v-else-if="foo">else-if</h3>
        assert_eq!(1, seq.else_if_nodes.len());
        check_else_if_node(&seq.else_if_nodes[0]);

        // <h3 v-else>else</h3>
        check_else_node(seq.else_node.as_ref());
    }

    #[test]
    fn it_folds_multiple_ifs() {
        // <template>
        //   <h1 v-if="true">if</h1>
        //   <h1 v-if="true">if</h1>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![if_node(), if_node()],
        };

        optimize_template(&mut sfc_template);

        // Template roots: two conditional sequences
        assert_eq!(2, sfc_template.roots.len());
        let Node::ConditionalSeq(ref seq) = sfc_template.roots[0] else {
            panic!("roots[0] is not a conditional sequence")
        };
        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);

        let Node::ConditionalSeq(ref seq) = sfc_template.roots[1] else {
            panic!("roots[1] not a conditional sequence")
        };
        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);
    }

    #[test]
    fn it_folds_multiple_else_ifs() {
        // <template>
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![if_node(), else_if_node(), if_node(), else_if_node()],
        };

        optimize_template(&mut sfc_template);

        // Template roots: two conditional sequences
        assert_eq!(2, sfc_template.roots.len());
        let Node::ConditionalSeq(ref seq) = sfc_template.roots[0] else {
            panic!("roots[0] is not a conditional sequence")
        };
        check_if_node(&seq.if_node);
        check_else_if_node(&seq.else_if_nodes[0]);

        let Node::ConditionalSeq(ref seq) = sfc_template.roots[1] else {
            panic!("roots[1] not a conditional sequence")
        };
        check_if_node(&seq.if_node);
        check_else_if_node(&seq.else_if_nodes[0]);
    }

    #[test]
    fn it_leaves_bad_nodes() {
        // <template>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h3 v-else>else</h3>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![else_if_node(), else_node()],
        };

        optimize_template(&mut sfc_template);

        // Template roots: still two
        assert_eq!(2, sfc_template.roots.len());
        assert!(matches!(sfc_template.roots[0], Node::Element(_)));
        assert!(matches!(sfc_template.roots[1], Node::Element(_)));
    }

    #[test]
    fn it_handles_complex_cases() {
        // <template><div>
        //   text
        //   <h1 v-if="true">if</h1>
        //   text
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   text
        //   <h1 v-if="true">if</h1>
        //   <h3 v-else>else</h3>
        // </div></template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    text_node(),
                    if_node(),
                    text_node(),
                    if_node(),
                    else_if_node(),
                    text_node(),
                    if_node(),
                    else_node(),
                ],
                template_scope: 0,
            })],
        };

        optimize_template(&mut sfc_template);

        // Template roots: one div
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref div) = sfc_template.roots[0] else {
            panic!("Root is not an element")
        };

        // Text and conditional seq
        assert_eq!(6, div.children.len());
        check_text_node(&div.children[0]);
        check_text_node(&div.children[2]);
        check_text_node(&div.children[4]);
        assert!(matches!(&div.children[1], Node::ConditionalSeq(_)));
        assert!(matches!(&div.children[3], Node::ConditionalSeq(_)));
        assert!(matches!(&div.children[5], Node::ConditionalSeq(_)));
    }

    #[test]
    fn it_ignores_node_without_conditional_directives() {
        let no_directives1 = Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "test-component",
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    ..Default::default()
                })),
            },
            children: vec![],
            template_scope: 0,
        });

        let no_directives2 = Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "div",
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("hello")],
            template_scope: 0,
        });

        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![no_directives1, no_directives2],
        };

        optimize_template(&mut sfc_template);

        // Template roots: both nodes are still present
        assert_eq!(2, sfc_template.roots.len());
    }

    // text
    fn text_node() -> Node<'static> {
        Node::Text("text")
    }

    fn check_text_node(node: &Node) {
        assert!(matches!(node, Node::Text("text")));
    }

    // <h1 v-if="true">if</h1>
    fn if_node() -> Node<'static> {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h1",
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_if: Some("true"),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("if")],
            template_scope: 0,
        })
    }

    fn check_if_node(if_node: &(&str, Box<ElementNode>)) {
        assert_eq!("true", if_node.0);
        assert!(matches!(
            *if_node.1,
            ElementNode {
                starting_tag: StartingTag { tag_name: "h1", .. },
                ..
            }
        ));
    }

    // <h2 v-else-if="foo">else-if</h3>
    fn else_if_node() -> Node<'static> {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h2",
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_else_if: Some("foo"),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("else-if")],
            template_scope: 0,
        })
    }

    fn check_else_if_node(else_if_node: &(&str, ElementNode)) {
        // condition, then node
        assert_eq!("foo", else_if_node.0);
        assert!(matches!(
            else_if_node.1,
            ElementNode {
                starting_tag: StartingTag { tag_name: "h2", .. },
                ..
            }
        ));
    }

    // <h3 v-else>else</h3>
    fn else_node() -> Node<'static> {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h3",
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("else")],
            template_scope: 0,
        })
    }

    fn check_else_node(else_node: Option<&Box<ElementNode>>) {
        let else_node = else_node.expect("Must have else node");
        assert!(matches!(
            **else_node,
            ElementNode {
                starting_tag: StartingTag { tag_name: "h3", .. },
                ..
            }
        ));
    }
}
