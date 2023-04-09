use crate::parser::structs::{StartingTag, Node};

use super::codegen::CodegenContext;
use super::directives::conditional::filter_nodes_with_conditional_directives;
use super::helper::CodeHelper;
use super::imports::VueImports;

impl <'a> CodegenContext <'a> {
  pub fn create_element_vnode(
    &mut self,
    buf: &mut String,
    element_node: &ElementNode,
    wrap_in_block: bool // for doing (openBlock(), createElementBlock(...))
  ) {
    let ElementNode { starting_tag, children, template_scope } = element_node;

    // Todo also add same logic to components
    let had_v_for = self.generate_vfor_prefix(buf, starting_tag);

    // Special generation: `_withDirectives` prefix
    let needs_directive = Self::needs_directive_wrapper(starting_tag, false);
    if needs_directive {
      buf.push_str(self.get_and_add_import_str(VueImports::WithDirectives));
      CodeHelper::open_paren(buf);
    }

    // Special generation: (openBlock(), createElementBlock(
    let should_wrap_in_block = wrap_in_block || had_v_for;
    if should_wrap_in_block {
      self.generate_create_element_block(buf);
    } else {
      buf.push_str(self.get_and_add_import_str(VueImports::CreateElementVNode));
      CodeHelper::open_paren(buf);
    }

    // Tag name
    CodeHelper::quoted(buf, starting_tag.tag_name);

    // Attributes
    CodeHelper::comma(buf);
    let has_generated_attributes = self.generate_attributes(buf, &starting_tag.attributes, true);
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

    // When the block was opened, we need to close the extra parenthesis
    if should_wrap_in_block {
      CodeHelper::close_paren(buf)
    }

    // Ending paren
    CodeHelper::close_paren(buf);

    // Generate directives array if needed
    if needs_directive {
      CodeHelper::comma(buf);
      self.generate_directives(buf, starting_tag, false);
      CodeHelper::close_paren(buf);
    }

    // Close v-for if it was there
    if had_v_for {
      self.generate_vfor_suffix(buf, starting_tag);
    }
  }

  pub fn generate_element_children(self: &mut Self, buf: &mut String, children: &[Node], allow_inlining: bool) -> bool {
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
          Node::DynamicExpression { .. } | Node::TextNode(_) => {
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

    // We can potentially inline if all the nodes are text nodes...
    let is_inlinable = if let Some((start, end)) = text_node_ranges.get(0) {
      *start == 0 && *end == children.len() - 1
    } else {
      false
    };

    // ... but only if explicitly allowed
    let should_inline = allow_inlining && is_inlinable;

    // Check if we need to spread the results over multiple lines
    let is_multiline = !is_inlinable && children.len() > 1;

    let mut children_iter = children.iter().enumerate();
    let mut text_node_ranges_iter = text_node_ranges.iter();
    let mut text_node_ranges_current = text_node_ranges_iter.next();

    // v-else nodes are generated only in pair with the previous v-if nodes
    // TODO optimize CommentNode + Node[v-else] -> <template v-else>CommentNode + Node</template>
    let mut conditional_nodes = filter_nodes_with_conditional_directives(children).peekable();
    let mut curr_conditional_node = conditional_nodes.peek();
    let mut curr_conditional_node_idx = curr_conditional_node.map_or(usize::MAX, |(idx, _)| *idx);

    // Start Js array
    if !should_inline {
      buf.push('[');
    }
    if is_multiline {
      self.code_helper.indent();
      self.code_helper.newline(buf);
    }

    // Create an expression for children
    while let Some((index, child)) = children_iter.next() {
      // Close previous node if any, add separator
      if index > 0 && is_multiline {
        self.code_helper.comma_newline(buf)
      } else if index > 0 {
        CodeHelper::comma(buf)
      }

      // Check if this is a conditional nodes sequence, process it separately
      if curr_conditional_node_idx == index {
        let nodes_processed = self.generate_consecutive_conditional_nodes(buf, &mut conditional_nodes);
        curr_conditional_node = conditional_nodes.peek();
        curr_conditional_node_idx = curr_conditional_node.map_or(usize::MAX, |(idx, _)| *idx);

        // Advance iterator
        for _ in 0..nodes_processed-1 {
          children_iter.next();
        }

        if nodes_processed > 0 {
          continue;
        }
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

      // This must never be the case!
      if let Node::TextNode(_) | Node::DynamicExpression { .. } = child {
        unreachable!("TextNode or DynamicExpression handled outside text_node_ranges");
      }

      // Make a call to handle a child
      self.compile_node(buf, child, false)
    }

    // Close Js array
    if is_multiline {
      self.code_helper.unindent();
      self.code_helper.newline(buf);
    }
    if !should_inline {
      buf.push(']');
    }

    true
  }

  fn create_text_concatenation_from_nodes(&mut self, buf: &mut String, nodes: &[Node], surround_with_create_text_vnode: bool) -> bool {
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
          let trimmed_text = escaped_text.trim();

          buf.push('"');
          if has_start_whitespace {
            buf.push(' ');
          }
          buf.push_str(trimmed_text);
          if has_end_whitespace && trimmed_text.len() > 0 {
            buf.push(' ');
          }
          buf.push('"');

          had_first_el = true;
        },

        /*
         * Transforms a dynamic expression into a `toDisplayString` call
         * Adds context to the variables from component scope
         */
        Node::DynamicExpression { value, .. } => {
          // todo add ctx reference depending on analysis
          buf.push_str(self.get_and_add_import_str(VueImports::ToDisplayString));
          buf.push('(');
          buf.push_str(&value);
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

  /// Generates `(openBlock(), createElementBlock(`
  #[inline]
  fn generate_create_element_block(&mut self, buf: &mut String) {
    CodeHelper::open_paren(buf);
    buf.push_str(self.get_and_add_import_str(VueImports::OpenBlock));
    CodeHelper::open_paren(buf);
    CodeHelper::close_paren(buf);
    CodeHelper::comma(buf);
    buf.push_str(self.get_and_add_import_str(VueImports::CreateElementBlock));
    CodeHelper::open_paren(buf);
  }
}
