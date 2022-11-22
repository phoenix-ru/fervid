use std::collections::HashSet;

use crate::parser::{Node, attributes::HtmlAttribute, StartingTag};

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

    /*
      In the block below, "text node" means Node::TextNode or Node::DynamicExpression, as both could be concatenated

      had_to_display_string is for `toDisplayString` import without doing HashSet::insert() for every DynamicExpression
      had_text_vnode is the same for `createTextVNode` import
      is_previous_node_text marks whether we could continue concatenating text nodes 
      current_text_start marks the start of current text nodes region, will be used if we suddenly encounter non-text node 
     */

    /* Prepare ranges of text nodes, i.e. vector of (start, end) indices for text nodes */
    let mut text_node_ranges: Vec<(usize, usize)> = vec![];
    {
      let mut start_of_range: Option<usize> = None;
      let mut end_of_range: Option<usize> = None;
      for (index, child) in children.iter().enumerate() {
        match child {
          Node::DynamicExpression(_) | Node::TextNode(_) => {
            if let None = start_of_range  {
              start_of_range = Some(index);
            }

            end_of_range = Some(index);
          },

          _ => {
            /* Close previous range */
            if let (Some(start), Some(end)) = (start_of_range, end_of_range) {
              text_node_ranges.push((start, end));
            }

            start_of_range = None;
            end_of_range = None;
          }
        }
      }

      if let (Some(start), Some(end)) = (start_of_range, end_of_range) {
        text_node_ranges.push((start, end))
      }
    };

    // println!("Text node ranges: {:?}", text_node_ranges);

    /* All nodes inside element are text or dynamic expressions `{{ }}` */
    let is_text_only = if let Some((start, end)) = text_node_ranges.get(0) {
      *start == 0 && *end == children.len() - 1
    } else { false };

    /* Create an expression for children */
    let mut children_str = String::new();
    let mut children_iter = children.iter().enumerate();
    let mut text_node_ranges_iter = text_node_ranges.iter();
    let mut text_node_ranges_current = text_node_ranges_iter.next();

    while let Some((index, child)) = children_iter.next() {
      /* Close previous text node if any, add separator */
      if !children_str.is_empty() {
        children_str.push_str(", ");
      }

      /* If we are at the start of multiple text nodes, i.e. (TextNode | DynamicExpression)+, pass control to another func */
      if let Some((start, end)) = text_node_ranges_current {
        if *start == index {
          let text_nodes = &children[*start..=*end];

          let text_nodes_result = self.create_text_concatenation_from_nodes(text_nodes, !is_text_only);

          children_str.push_str(&text_nodes_result);

          /* Advance iterators forward */
          text_node_ranges_current = text_node_ranges_iter.next();
          for _ in 0..(*end - *start) {
            children_iter.next();
          }

          continue;
        }
      }

      match child {
        Node::ElementNode { starting_tag, children } => {
          // todo use real differentiation for node VS component
          let child_result = self.create_element_vnode(starting_tag, children);
          children_str.push_str(child_result.as_str());
        },

        _ => {
          panic!("TextNode or DynamicExpression handled outside text_node_ranges");
        }
      }
    }

    /* Make an array from children (if not text nodes only), default to `null` */
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

    format!("_createElementVNode(\"{}\", {}, {}, {})", tag_name, attributes_str, children_str, mode)
  }

  fn create_text_concatenation_from_nodes(self: &mut Self, nodes: &[Node], surround_with_create_text_vnode: bool) -> String {
    let mut result = String::new();

    /* Add function call if asked */
    if surround_with_create_text_vnode {
      result.push_str("_createTextVNode(");
      self.add_to_imports("_createTextVNode");
    }

    /* Adding to imports */
    let mut had_to_display_string = false;

    /* Just in case this function is called with wrong Node slice */
    let mut had_first_el = false;

    for node in nodes {
      if had_first_el {
        result.push_str(" + ");
      }

      match node {
        /*
         * Transforms raw text content into a JavaScript string
         * All the start and end whitespace would be trimmed and replaced by a single regular space ` `
         * All double quotes in the string are escaped with `\"`
         */
        Node::TextNode(v) => {
          let escaped_text = v.replace('"', "\\\"");
          let has_start_whitespace = escaped_text.starts_with(char::is_whitespace);
          let has_end_whitespace = escaped_text.ends_with(char::is_whitespace);

          result.push('"');
          if has_start_whitespace {
            result.push(' ');
          }
          result.push_str(escaped_text.trim());
          if has_end_whitespace {
            result.push(' ');
          }
          result.push('"');

          had_first_el = true;
        },

        /*
         * Transforms a dynamic expression into a `toDisplayString` call
         * Adds context to the variables from component scope
         */
        Node::DynamicExpression(v) => {
          // todo add ctx reference depending on analysis
          result.push_str("_toDisplayString(");
          result.push_str(v);
          result.push(')');

          had_first_el = true;
          had_to_display_string = true;
        },

        Node::ElementNode { .. } => {
          // ????
        }
      }
    }

    if surround_with_create_text_vnode {
      result.push_str(", 1 /* TEXT */)");
    }

    if had_to_display_string {
      self.add_to_imports("_toDisplayString");
    }

    result
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
  }
}
