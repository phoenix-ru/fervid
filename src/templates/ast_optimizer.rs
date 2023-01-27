use crate::parser::structs::{Node, ElementNode};

pub fn optimize_ast <'a> (ast: &'a mut [Node]) -> &'a [Node<'a>] {
  let mut ast_optimizer = AstOptimizer;

  let mut iter = ast.iter_mut();
  while let Some(ref mut node) = iter.next() {
    node.visit_mut_with(&mut ast_optimizer);   
  }

  ast
}

struct AstOptimizer;

trait VisitMutWith {
  fn visit_element_node(&mut self, element_node: &mut ElementNode);
}

impl <'a> VisitMutWith for AstOptimizer {
  fn visit_element_node(&mut self, element_node: &mut ElementNode) {
    // Because indices are `usize`, we can't use -1, therefore actual indices would start at 1
    // and end at `children_len`. Kind of okay for this scenario to avoid using i32.
    let mut index = 0;
    let children_len = element_node.children.len();

    // Filter out whitespace text nodes at the beginning and end of ElementNode;
    element_node.children.retain(|child| {
      index += 1;

      match child {
        // First and last children are whitespace text nodes, we don't need them
        Node::TextNode(v)
        if (index == 1 || index == children_len) && (v.trim().len() == 0) => {
          false
        },

        _ => true
      }
    });

    if let Some(Node::TextNode(text_node)) = element_node.children.last() {
      if text_node.trim().len() == 0 {
        element_node.children.pop();
      }
    }

    element_node.visit_mut_children_with(self);
  }
}

impl Node<'_> {
  fn visit_mut_with(&mut self, visitor: &mut impl VisitMutWith) {
    match self {
      Node::ElementNode(el) => { el.visit_mut_with(visitor) },
      _ => {}
    }
  }
}

impl ElementNode<'_> {
  fn visit_mut_children_with(&mut self, visitor: &mut impl VisitMutWith) {
    for child in &mut self.children {
      child.visit_mut_with(visitor)
    }
  }

  fn visit_mut_with(&mut self, visitor: &mut impl VisitMutWith) {
    visitor.visit_element_node(self);
  }
}
