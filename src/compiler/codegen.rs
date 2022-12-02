extern crate regex;

use std::collections::HashMap;
use regex::Regex;

use crate::parser::{Node, attributes::HtmlAttribute, StartingTag};
use super::all_html_tags::is_html_tag;

#[derive(Default)]
pub struct CodegenContext <'a> {
  pub components: HashMap<&'a str, String>,
  pub used_imports: u64,
  hoists: Vec<String>,
  is_custom_element: IsCustomElementParam<'a>
}

enum IsCustomElementParamRaw <'a> {
  String(&'a str),
  Regex(&'a str),
  None
}

enum IsCustomElementParam <'a> {
  String(&'a str),
  Regex(Regex),
  None
}

impl Default for IsCustomElementParam<'_> {
  fn default() -> Self {
    Self::None
  }
}

/**
 * Main entry point for the code generation
 */
pub fn compile_template(template: Node) -> Result<String, i32> {
  // Todo from options
  let is_custom_element_param = IsCustomElementParamRaw::Regex("custom-");
  let is_custom_element_re = match is_custom_element_param {
    IsCustomElementParamRaw::Regex(re) => IsCustomElementParam::Regex(Regex::new(re).expect("Invalid isCustomElement regex")),
    IsCustomElementParamRaw::String(string) => IsCustomElementParam::String(string),
    IsCustomElementParamRaw::None => IsCustomElementParam::None
  };

  /* Create the context */
  let mut ctx: CodegenContext = Default::default();
  ctx.is_custom_element = is_custom_element_re;

  let test_res = ctx.compile_node(&template);

  // Debug info: show used imports
  println!("Used imports: {}", ctx.generate_imports_string());
  println!();

  // Debug info: show used imports
  println!("Used components: {:?}", ctx.components);
  println!();

  // Zero really means nothing. It's just that error handling is not yet implemented
  test_res.ok_or(0)
}

impl <'a> CodegenContext <'a> {
  pub fn compile_node(self: &mut Self, node: &'a Node) -> Option<String> {
    match node {
      Node::ElementNode { starting_tag, children } => {
        let mut buf = String::new();
        self.create_element_vnode(&mut buf, starting_tag, children);
        Some(buf)
      },
      _ => None
    }
  }

  pub fn compile_node_children(self: &mut Self, children: &Vec<Node>) {
    todo!()
  }

  fn add_to_hoists(self: &mut Self, expression: String) -> String {
    let hoist_index = self.hoists.len() + 1;
    let hoist_identifier = format!("_hoisted_{}", hoist_index);

    // todo add pure in consumer instead or provide a boolean flag to generate it
    let hoist_expr = format!("const {} = /*#__PURE__*/ {}", hoist_identifier, expression);
    self.hoists.push(hoist_expr);

    hoist_identifier
  }

  pub fn is_component(self: &Self, starting_tag: &StartingTag) -> bool {
    // todo use analyzed components? (fields of `components: {}`)

    let tag_name = starting_tag.tag_name;

    let is_html_tag = is_html_tag(tag_name);
    if is_html_tag {
      return false;
    }

    /* Check with isCustomElement */
    let is_custom_element = match &self.is_custom_element {
      IsCustomElementParam::String(string) => tag_name == *string,
      IsCustomElementParam::Regex(re) => re.is_match(tag_name),
      IsCustomElementParam::None => false
    };

    !is_custom_element
  }

  /**
   * The element can be hoisted if it and all of its descendants do not have dynamic attributes
   * <div class="a"><span>text</span></div> => true
   * <button :disabled="isDisabled">text</button> => false
   * <span>{{ text }}</span> => false
   */
  fn can_be_hoisted (self: &Self, node: &Node) -> bool {
    match node {
      Node::ElementNode { starting_tag, children } => { 
        /* Check starting tag */
        if self.is_component(starting_tag) {
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

        let cannot_be_hoisted = children.iter().any(|it| !self.can_be_hoisted(&it));

        return !cannot_be_hoisted;
      },

      Node::TextNode(_) => {
        return true
      },

      Node::DynamicExpression(_) => {
        return false
      }
    };
  }
}
