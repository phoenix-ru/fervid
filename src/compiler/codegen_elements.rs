use crate::parser::{Node, StartingTag};
use super::codegen::CodegenContext;
use super::helper::CodeHelper;
use super::imports::VueImports;

impl <'a> CodegenContext <'a> {
  pub fn create_element_vnode(self: &mut Self, buf: &mut String, starting_tag: &StartingTag, children: &'a [Node]) {
    buf.push_str(self.get_and_add_import_str(VueImports::CreateElementVNode));
    CodeHelper::open_paren(buf);

    // Tag name
    buf.push('"');
    buf.push_str(starting_tag.tag_name);
    buf.push('"');

    // Attributes
    CodeHelper::comma(buf);
    let has_generated_attributes = self.generate_attributes(buf, &starting_tag.attributes);
    if !has_generated_attributes {
      buf.push_str("null");
    }

    // Children
    CodeHelper::comma(buf);
    let has_generated_children = self.generate_element_children(buf, children, true);
    if !has_generated_children {
      buf.push_str("null");
    }

    // todo implement MODE
    // todo add /*#__PURE__*/ annotations
    CodeHelper::comma(buf);
    buf.push_str("-1 /* HOISTED */");

    // Ending paren
    CodeHelper::close_paren(buf)
  }

  pub fn generate_element_children(self: &mut Self, buf: &mut String, children: &'a [Node], allow_inlining: bool) -> bool {
    // Do no work if children vec is empty
    if children.len() == 0 {
      return false;
    }

    /*
      In the code below, "text node" means Node::TextNode or Node::DynamicExpression, as both could be concatenated.
      This code is meant for inlining optimization.
      It means that if all passed children are text nodes and `allow_inlining` flag is true,
      we will concat them using Js plus `+` operator.

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

    let should_inline = allow_inlining && {
      if let Some((start, end)) = text_node_ranges.get(0) {
        *start == 0 && *end == children.len() - 1
      } else {
        false
      }
    };

    let mut children_iter = children.iter().enumerate();
    let mut text_node_ranges_iter = text_node_ranges.iter();
    let mut text_node_ranges_current = text_node_ranges_iter.next();

    // Start Js array
    if !should_inline {
      buf.push('[');
    }

    /* Create an expression for children */
    while let Some((index, child)) = children_iter.next() {
      /* Close previous text node if any, add separator */
      if index > 0 {
        buf.push_str(", ");
      }

      /* If we are at the start of multiple text nodes, i.e. (TextNode | DynamicExpression)+, pass control to another func */
      if let Some((start, end)) = text_node_ranges_current {
        if *start == index {
          let text_nodes = &children[*start..=*end];

          self.create_text_concatenation_from_nodes(buf, text_nodes, !should_inline);

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
          if self.is_component(starting_tag) {
            self.create_component_vnode(buf, starting_tag, children)
          } else {
            self.create_element_vnode(buf, starting_tag, children)
          }
        },

        Node::CommentNode(_) => {},

        _ => {
          panic!("TextNode or DynamicExpression handled outside text_node_ranges");
        }
      }
    }

    // Close Js array
    if !should_inline {
      buf.push(']');
    }

    true
  }

  pub fn create_text_concatenation_from_nodes(self: &mut Self, buf: &mut String, nodes: &[Node], surround_with_create_text_vnode: bool) -> bool {
    /* Add function call if asked */
    if surround_with_create_text_vnode {
      buf.push_str(self.get_and_add_import_str(VueImports::CreateTextVNode));
      buf.push('(');
    }

    /* Just in case this function is called with wrong Node slice */
    let mut had_first_el = false;

    for node in nodes {
      if had_first_el {
        buf.push_str(" + ");
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

          buf.push('"');
          if has_start_whitespace {
            buf.push(' ');
          }
          buf.push_str(escaped_text.trim());
          if has_end_whitespace {
            buf.push(' ');
          }
          buf.push('"');

          had_first_el = true;
        },

        /*
         * Transforms a dynamic expression into a `toDisplayString` call
         * Adds context to the variables from component scope
         */
        Node::DynamicExpression(v) => {
          // todo add ctx reference depending on analysis
          buf.push_str(self.get_and_add_import_str(VueImports::ToDisplayString));
          buf.push('(');
          buf.push_str(v);
          buf.push(')');

          had_first_el = true;
        },

        Node::ElementNode { .. } | Node::CommentNode(_) => {
          // ????
        }
      }
    }

    if surround_with_create_text_vnode {
      buf.push_str(", 1 /* TEXT */)");
    }

    true
  }
}
