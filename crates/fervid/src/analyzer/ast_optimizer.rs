use fervid_core::{Node, ElementNode, StartingTag, HtmlAttribute, VDirective, SfcTemplateBlock};

use crate::compiler::all_html_tags;

pub fn optimize_template<'a> (template: &'a mut SfcTemplateBlock) -> &'a SfcTemplateBlock<'a> {
  let mut ast_optimizer = AstOptimizer;

  // Only retain `ElementNode`s as template roots
  template.roots.retain(|root| matches!(root, Node::ElementNode(_)));

  let ast = &mut template.roots;
  let mut iter = ast.iter_mut();
  while let Some(ref mut node) = iter.next() {
    node.visit_mut_with(&mut ast_optimizer);   
  }

  template
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

impl <'a> Visitor for AstOptimizer {
  fn visit_element_node(&mut self, element_node: &mut ElementNode) {
    let children_len = element_node.children.len();

    // Discard children mask, limited to 128 children. 0 means to preserve the node, 1 to discard
    let mut discard_mask: u128 = 0;

    // Filter out whitespace text nodes at the beginning and end of ElementNode
    match element_node.children.first() {
      Some(Node::TextNode(v)) if v.trim().len() == 0 => {
        discard_mask |= 1<<0;
      }
      _ => {}
    }
    match element_node.children.last() {
      Some(Node::TextNode(v)) if v.trim().len() == 0 => {
        discard_mask |= 1<<(children_len-1);
      }
      _ => {}
    }

    // For removing the middle whitespace text nodes, we need sliding windows of three nodes
    for (index, window) in element_node.children.windows(3).enumerate() {
      match window {
        [Node::ElementNode(_) | Node::CommentNode(_), Node::TextNode(middle), Node::ElementNode(_) | Node::CommentNode(_)]
        if middle.trim().len() == 0 => {
          discard_mask |= 1<<(index + 1); 
        },
        _ => {}
      }
    }

    // Retain based on discard_mask. If a discard bit at `index` is set to 1, the node will be dropped
    let mut index = 0;
    element_node.children.retain(|_| {
      let should_retain = discard_mask & (1<<index) == 0;
      index += 1;
      should_retain
    });

    // For components, reorder children so that named slots come first
    if self.is_component(&element_node.starting_tag) && element_node.children.len() > 0 {
      element_node.children.sort_by(|a, b| {
        let a_is_from_default = is_from_default_slot(a);
        let b_is_from_default = is_from_default_slot(b);

        a_is_from_default.cmp(&b_is_from_default)
      });
    }

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
      Node::ElementNode(el) => { el.visit_mut_with(visitor) },
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
  let Node::ElementNode(ElementNode { starting_tag, .. }) = node else {
    return true;
  };

  if starting_tag.tag_name != "template" {
    return true;
  }

  // Slot is not default if its `v-slot` has an argument which is not "" or "default"
  // `v-slot` is default
  // `v-slot:default` is default
  // `v-slot:custom` is not default
  !starting_tag
    .attributes
    .iter()
    .any(|attr| match attr {
      HtmlAttribute::VDirective (VDirective { name, argument, .. }) => {
        *name == "slot" && *argument != "" && *argument != "default"
      },

      HtmlAttribute::Regular { .. } => false
    })
}
