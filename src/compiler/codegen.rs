use std::collections::HashSet;

use crate::parser::Node;

struct CodegenContext <'a> {
  used_imports: HashSet<&'a str>
}

pub fn compile_template(template: Node) -> String {
  let result = String::new();

  let mut ctx = CodegenContext {
    used_imports: Default::default()
  };

  let test_res = ctx.create_element_vnode(template);

  match test_res {
    Some(res) => res,
    None => result
  }
}

impl <'a> CodegenContext <'a> {
  pub fn create_element_vnode(self: &mut Self, node: Node) -> Option<String> {
    match node {
      Node::ElementNode { starting_tag, children } => {
        // todo handle this case
        if children.len() != 1 {
          return None;
        }

        // Element node with only text inside
        if let Some(Node::TextNode(contents)) = children.get(0) {
          self.add_to_imports("_createElementVNode");

          return Some(String::from(
            format!("_createElementVNode('{}', null, '{}', -1 /* HOISTED */)", starting_tag.tag_name, contents)
          ));
        };

        // todo

        None
      },

      _ => None
    }
  }

  fn add_to_imports(self: &mut Self, import: &'a str) {
    if self.used_imports.contains(import) {
      return;
    }
    self.used_imports.insert(import);
  }
}
