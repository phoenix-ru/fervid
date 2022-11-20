use std::collections::HashSet;

use crate::parser::{Node, self, attributes::HtmlAttribute, StartingTag};

#[derive(Default)]
struct CodegenContext <'a> {
  used_imports: HashSet<&'a str>,
  hoists: Vec<String>
}

pub fn compile_template(template: Node) -> String {
  let result = String::new();

  let mut ctx: CodegenContext = Default::default();

  let test_res = ctx.compile_node(&template);

  match test_res {
    Some(res) => res,
    None => result
  }
}

impl <'a> CodegenContext <'a> {
  pub fn compile_node(self: &mut Self, node: &Node) -> Option<String> {
    match node {
        Node::ElementNode { starting_tag, children } => Some(self.create_element_vnode(starting_tag, children)),
        _ => None
    }
  }

  pub fn compile_node_children(self: &mut Self, children: &Vec<Node>) {

  }

  pub fn create_element_vnode(self: &mut Self, starting_tag: &StartingTag, children: &Vec<Node>) -> String {
    let tag_name = starting_tag.tag_name;
    let attributes = &starting_tag.attributes;

    /* Create an expression for attributes */
    let mut attributes_str = String::new();
    for attribute in attributes {
      match attribute {
        HtmlAttribute::Regular { name, value } => {
          if !attributes_str.is_empty() { attributes_str.push(','); };

          let needs_quotes = name.contains('-');
          let normalized_value = if needs_quotes {
            format!("\"{}\": \"{}\"", name, value)
          } else {
            format!("{}: \"{}\"", name, value)
          };

          // todo special case for `style` attribute
          attributes_str.push_str(normalized_value.as_str());
        },

        // todo what should be done to that??
        HtmlAttribute::VDirective { .. } => {}
      };
    }

    /* Make an object out of attrs expression, otherwise default to `null` */
    if attributes_str.is_empty() {
      attributes_str.push_str("null");
    } else {
      attributes_str.insert(0, '{');
      attributes_str.push('}');
    }

    /* All nodes inside element are text or dynamic expressions `{{ }}` */
    let is_text_only = children.iter().all(|it| {
      match it {
        Node::TextNode(_) => true,
        Node::DynamicExpression(_) => true,
        Node::ElementNode { .. } => false
      }
    });

    /*
      sep_str is separator between children. Text nodes are concatted, otherwise an array is used.
      had_dynamic_expression is for `toDisplayString` import without doing HashSet::insert() for every DynamicExpression
     */
    let sep_str = if is_text_only { " + " } else { ", " };
    let mut had_dynamic_expression = false;

    /* Create an expression for children */
    let mut children_str = String::new();
    for child in children {
      if !children_str.is_empty() { children_str.push_str(sep_str) };

      if is_text_only {
        /* Concat text */
        match child {
            Node::TextNode(v) => {
              // todo trim extra whitespace
              let escaped_text = v.replace('"', "\\\"");
              children_str.push('"');
              children_str.push_str(escaped_text.as_str());
              children_str.push('"');
            },

            Node::DynamicExpression(v) => {
              had_dynamic_expression = true;
              // todo add ctx reference
              children_str.push_str("_toDisplayString(");
              children_str.push_str(v);
              children_str.push(')');
            },

            _ => {}
        }
      } else {
        /* Create node */
      }
    }

    /* Make an array from children, default to `null` */
    if children_str.is_empty() {
      children_str.push_str("null");
    } else if !is_text_only {
      children_str.insert(0, '[');
      children_str.push(']');
    }

    // todo implement
    // todo add /*#__PURE__*/ annotations
    let mode = "-1 /* HOISTED */";

    /* Add imports */
    self.add_to_imports("_createElementVNode");
    if had_dynamic_expression {
      self.add_to_imports("_toDisplayString");
    }

    format!("_createElementVNode(\"{}\", {}, {}, {})", tag_name, attributes_str, children_str, mode)
  }

  pub fn create_element_vnode2(self: &mut Self, node: &Node) -> Option<String> {
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
    self.used_imports.insert(import);
  }

  fn add_to_hoists(self: &mut Self, expression: String) -> String {
    let hoist_index = self.hoists.len() + 1;
    let hoist_identifier = format!("_hoisted_{}", hoist_index);

    let hoist_expr = format!("const {} = /*#__PURE__*/ {}", hoist_identifier, expression);
    self.hoists.push(hoist_expr);

    hoist_identifier
  }

  fn is_native_element(node: &Node) -> bool {
    match node {
        Node::ElementNode { starting_tag, .. } => {
          // todo check for component nodes (may be tricky, because names do not always follow rules...)
          // todo use analyzed components (fields of `components: {}`)
          // todo check with isCustomElement

          !starting_tag.tag_name.contains('-')
        },

        _ => false
    }
  }

  /**
   * The element can be hoisted if it and all of its descendants do not have dynamic attributes
   * <div class="a"><span>text</span></div> => true
   * <button :disabled="isDisabled">text</button> => false
   * <span>{{ text }}</span> => false
   */
  fn can_be_hoisted (node: &Node) -> bool {
    match node {
        Node::ElementNode { starting_tag, children } => { 
          /* Check starting tag */
          if !Self::is_native_element(node) {
            return false;
          }

          let has_any_dynamic_attr = starting_tag.attributes.iter().any(|it| {
            match it {
              HtmlAttribute::Regular { .. } => false,
              HtmlAttribute::VDirective { .. } => true
            }
          });

          if has_any_dynamic_attr {
            return false;
          }

          let cannot_be_hoisted = children.iter().any(|it| !Self::can_be_hoisted(&it));

          return !cannot_be_hoisted;
        },

        Node::TextNode(_) => {
          return true
        },

        Node::DynamicExpression(_) => {
          return false
        }
    };

    // if let Node::ElementNode { starting_tag, children } = node {
    //   let first_child = children.get(0);

    //   if let None = first_child {
    //     return true
    //   } else if let Some(Node::TextNode(_)) = first_child {
    //     return true
    //   }

      
    // }

    // false
  }
}
