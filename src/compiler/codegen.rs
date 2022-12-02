extern crate regex;

use std::collections::{HashSet, HashMap};
use regex::Regex;

use crate::parser::{Node, attributes::HtmlAttribute, StartingTag};
use super::{all_html_tags::is_html_tag, imports::VueImports};

#[derive(Default)]
pub struct CodegenContext <'a> {
  components: HashMap<&'a str, String>,
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

  // Zero really means nothing. It's just that error handling is not yet implemented
  test_res.ok_or(0)
}

impl <'a> CodegenContext <'a> {
  pub fn compile_node(self: &mut Self, node: &Node) -> Option<String> {
    match node {
        Node::ElementNode { starting_tag, children } => Some(self.create_element_vnode(starting_tag, children)),
        _ => None
    }
  }

  pub fn compile_node_children(self: &mut Self, children: &Vec<Node>) {
    todo!()
  }

  pub fn create_element_vnode(self: &mut Self, starting_tag: &StartingTag, children: &Vec<Node>) -> String {
    /* Result buffer */
    let mut buf = String::from(self.get_and_add_import_str(VueImports::CreateElementVNode));
    buf.push('(');

    // Tag name
    buf.push('"');
    buf.push_str(starting_tag.tag_name);
    buf.push('"');

    // Attributes
    buf.push_str(", ");
    let has_generated_attributes = self.generate_attributes(&mut buf, &starting_tag.attributes);
    if !has_generated_attributes {
      buf.push_str("null");
    }

    // Children
    buf.push_str(", ");
    let has_generated_children = self.generate_element_children(&mut buf, children, true);
    if !has_generated_children {
      buf.push_str("null");
    }

    // todo implement MODE
    // todo add /*#__PURE__*/ annotations
    buf.push_str(", ");
    buf.push_str("-1 /* HOISTED */");

    // Ending paren
    buf.push(')');

    buf
  }

  pub fn create_component_vnode(self: &mut Self, starting_tag: &StartingTag, children: &Vec<Node>) -> String {
    todo!()
  }

  fn add_to_components(self: &mut Self, tag_name: &'a str) -> String {
    /* Check component existence and early exit */
    let existing_component_name = self.components.get(tag_name);
    if let Some(component_name) = existing_component_name {
      return component_name.clone();
    }

    /* _component_ prefix plus tag name */
    let mut component_name = tag_name.replace('-', "_");
    component_name.insert_str(0, "_component_");

    /* Add to map */
    self.components.insert(tag_name, component_name.clone());

    component_name
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

    // todo remove checking for dash
    !is_custom_element && tag_name.contains('-')
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
