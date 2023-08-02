use fervid_core::{
    is_from_default_slot, is_html_tag, AttributeOrBinding, Conditional, ConditionalNodeSequence,
    ElementKind, ElementNode, Interpolation, Node, SfcTemplateBlock, StartingTag, VOnDirective,
    VSlotDirective, VUE_BUILTINS,
};
use smallvec::SmallVec;

use crate::structs::{ScopeHelper, TemplateScope};

use super::collect_vars::collect_variables;

struct TemplateVisitor<'s> {
    scope_helper: &'s mut ScopeHelper,
    current_scope: u32,
}

pub fn transform_and_record_template(
    template: &mut SfcTemplateBlock,
    scope_helper: &mut ScopeHelper,
) {
    // Only retain `ElementNode`s as template roots
    template
        .roots
        .retain(|root| matches!(root, Node::Element(_)));

    // Optimize conditional sequences within template root
    optimize_children(&mut template.roots, ElementKind::Element);

    // Todo merge 1+ children into a separate `<template>` element so that Fragment gets generated

    let mut template_visitor = TemplateVisitor {
        scope_helper,
        current_scope: 0,
    };

    // Optimize each root node separately
    let ast = &mut template.roots;
    let mut iter = ast.iter_mut();
    while let Some(ref mut node) = iter.next() {
        node.visit_mut_with(&mut template_visitor);
    }
}

/// Optimizes the children by removing whitespace in between `ElementNode`s,
/// as well as folding `v-if`/`v-else-if`/`v-else` sequences into a `ConditionalNodeSequence`
fn optimize_children(children: &mut Vec<Node>, element_kind: ElementKind) {
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
    if matches!(element_kind, ElementKind::Component) && children.len() > 0 {
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

        for mut child in children.drain(..) {
            // Only process `ElementNode`s.
            // Otherwise, when we have an `if` node, ignore `Comment`s and finish sequence.
            let Node::Element(child_element) = &mut child else {
                if let (Node::Comment(_), Some(_)) = (&child, seq.as_ref()) {
                    continue;
                } else {
                    finish_seq!(child);
                    continue;
                }
            };

            let Some(ref mut directives) = child_element.starting_tag.directives else {
                finish_seq!(child);
                continue;
            };

            // Check if we have a `v-if`.
            // The already existing sequence should end, and the new sequence should start.
            if let Some(v_if) = directives.v_if.take() {
                finish_seq!();
                seq = Some(ConditionalNodeSequence {
                    if_node: Box::new(Conditional {
                        condition: *v_if,
                        node: deref_element!(child),
                    }),
                    else_if_nodes: vec![],
                    else_node: None,
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

                seq.else_if_nodes.push(Conditional {
                    condition: *v_else_if,
                    node: deref_element!(child),
                });
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

trait Visitor {
    fn visit_element_node(&mut self, element_node: &mut ElementNode);
    fn visit_conditional_node(&mut self, conditional_node: &mut ConditionalNodeSequence);
    fn visit_interpolation(&mut self, interpolation: &mut Interpolation);
}

trait VisitMut {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor);
}

impl<'a> Visitor for TemplateVisitor<'_> {
    fn visit_element_node(&mut self, element_node: &mut ElementNode) {
        let parent_scope = self.current_scope;
        let mut scope_to_use = parent_scope;

        // Check if there is a scoping directive
        // Finds a `v-for` or `v-slot` directive when in ElementNode
        // and collects their variables into the new template scope
        if let Some(ref mut directives) = element_node.starting_tag.directives {
            let v_for = directives.v_for.as_mut();
            let v_slot = directives.v_slot.as_ref();

            // Create a new scope
            if v_for.is_some() || v_slot.is_some() {
                // New scope will have ID equal to length
                scope_to_use = self.scope_helper.template_scopes.len() as u32;
                self.scope_helper.template_scopes.push(TemplateScope {
                    variables: SmallVec::new(),
                    parent: parent_scope,
                });
            }

            if let Some(v_for) = v_for {
                // Get the iterator variable and collect its variables
                let mut scope = &mut self.scope_helper.template_scopes[scope_to_use as usize];
                collect_variables(&v_for.itervar, &mut scope);

                // Transform the iterable
                self.scope_helper
                    .transform_expr(&mut v_for.iterable, scope_to_use);
            }

            if let Some(VSlotDirective {
                value: Some(v_slot_value),
                ..
            }) = v_slot
            {
                // Collect slot bindings
                let mut scope = &mut self.scope_helper.template_scopes[scope_to_use as usize];
                collect_variables(v_slot_value, &mut scope);
                // TODO transform slot?
            }
        }

        // Update the element's scope and the Visitor's current scope
        element_node.template_scope = scope_to_use;
        self.current_scope = scope_to_use;

        // Transform the VBind and VOn attributes
        for attr in element_node.starting_tag.attributes.iter_mut() {
            match attr {
                AttributeOrBinding::VBind(v_bind) => {
                    self.scope_helper
                        .transform_expr(&mut v_bind.value, scope_to_use);
                }
                AttributeOrBinding::VOn(VOnDirective {
                    handler: Some(ref mut handler),
                    ..
                }) => {
                    self.scope_helper.transform_expr(handler, scope_to_use);
                }
                _ => {}
            }
        }

        // Transform the directives
        if let Some(ref mut directives) = element_node.starting_tag.directives {
            macro_rules! maybe_transform {
                ($key: ident) => {
                    match directives.$key.as_mut() {
                        Some(expr) => self.scope_helper.transform_expr(expr, scope_to_use),
                        None => false,
                    }
                };
            }
            maybe_transform!(v_html);
            maybe_transform!(v_memo);
            maybe_transform!(v_show);
            maybe_transform!(v_text);
        }

        // Mark the node with a correct type (element, component or built-in)
        let element_kind = self.recognize_element_kind(&element_node.starting_tag);
        element_node.kind = element_kind;

        // Merge conditional nodes and clean up whitespace
        optimize_children(&mut element_node.children, element_kind);

        // Recursively visit children
        for child in element_node.children.iter_mut() {
            child.visit_mut_with(self)
        }

        // Restore the parent scope
        self.current_scope = parent_scope;
    }

    fn visit_conditional_node(&mut self, conditional_node: &mut ConditionalNodeSequence) {
        // In this function, conditions are transformed first
        // without updating the template scope and collecting its variables.
        // I believe this is a correct way of doing it, because in VDOM the condition
        // wraps around the node (`condition ? if_node : else_node`).
        // However, I am not too sure about the `v-if` & `v-slot` combined usage.

        self.scope_helper
            .transform_expr(&mut conditional_node.if_node.condition, self.current_scope);
        self.visit_element_node(&mut conditional_node.if_node.node);

        for else_if_node in conditional_node.else_if_nodes.iter_mut() {
            self.scope_helper
                .transform_expr(&mut else_if_node.condition, self.current_scope);
            self.visit_element_node(&mut else_if_node.node);
        }

        if let Some(ref mut else_node) = conditional_node.else_node {
            self.visit_element_node(else_node);
        }
    }

    fn visit_interpolation(&mut self, interpolation: &mut Interpolation) {
        interpolation.template_scope = self.current_scope;

        let has_js = self
            .scope_helper
            .transform_expr(&mut interpolation.value, self.current_scope);

        interpolation.patch_flag = has_js;
    }
}

impl TemplateVisitor<'_> {
    fn recognize_element_kind(&self, starting_tag: &StartingTag) -> ElementKind {
        let tag_name = starting_tag.tag_name;

        // First, check for a built-in
        if let Some(builtin_type) = VUE_BUILTINS.get(tag_name) {
            return ElementKind::Builtin(*builtin_type);
        }

        // Then check if this is an HTML tag
        if is_html_tag(starting_tag.tag_name) {
            ElementKind::Element
        } else {
            ElementKind::Component
        }
    }
}

impl VisitMut for Node<'_> {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor) {
        match self {
            Node::Element(el) => visitor.visit_element_node(el),
            Node::ConditionalSeq(cond) => visitor.visit_conditional_node(cond),
            Node::Interpolation(interpolation) => visitor.visit_interpolation(interpolation),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{ElementKind, Node, VueDirectives};
    use swc_core::ecma::ast::Expr;

    use crate::test_utils::{parser::parse_javascript_expr, to_str};

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
                kind: ElementKind::Element,
            })],
        };

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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
                kind: ElementKind::Element,
            })],
        };

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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
            kind: ElementKind::Element,
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
            kind: ElementKind::Element,
        });

        let mut sfc_template = SfcTemplateBlock {
            lang: "html",
            roots: vec![no_directives1, no_directives2],
        };

        transform_and_record_template(&mut sfc_template, &mut Default::default());

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
                    v_if: Some(js("true")),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("if")],
            template_scope: 0,
            kind: ElementKind::Element,
        })
    }

    fn check_if_node(if_node: &Conditional) {
        assert_eq!("true", to_str(&if_node.condition));
        assert!(matches!(
            if_node.node,
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
                    v_else_if: Some(js("foo")),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("else-if")],
            template_scope: 0,
            kind: ElementKind::Element,
        })
    }

    fn check_else_if_node(else_if_node: &Conditional) {
        // condition, then node
        assert_eq!("_ctx.foo", to_str(&else_if_node.condition));
        assert!(matches!(
            else_if_node.node,
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
            kind: ElementKind::Element,
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

    fn js(raw: &str) -> Box<Expr> {
        parse_javascript_expr(raw, 0, Default::default()).unwrap().0
    }
}
