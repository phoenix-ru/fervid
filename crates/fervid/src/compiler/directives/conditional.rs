use std::iter::Peekable;

use fervid_core::Node;

use crate::compiler::{codegen::CodegenContext, helper::CodeHelper, imports::VueImports};

impl CodegenContext<'_> {
    /// Generates a sequence of `v-if` / `v-else-if` / `v-else` nodes
    /// Returns a number of processed conditional nodes
    pub fn generate_consecutive_conditional_nodes<'n>(
        &mut self,
        buf: &mut String,
        nodes: &mut Peekable<impl Iterator<Item = (usize, ConditionalNode<'n>)>>,
    ) -> usize {
        let mut nodes_generated = 0;
        let mut curr_index: usize = 0;
        let mut newlines_count = 0;
        let mut had_v_else = false;

        while let Some((index, ref conditional_node)) = nodes.next() {
            // Generate first node: this must always be `v-if`.
            // In the case it's not (which must be impossible after optimizer pass),
            // the conditional node will be consumed in the iterator but code will not generated
            // and number of processed items would be returned as 0
            if curr_index == 0 {
                if let ConditionalNode::VIf { node, condition } = conditional_node {
                    self.generate_if_node(buf, node, condition);
                    curr_index = index;
                    nodes_generated += 1;
                    newlines_count += 1;
                } else {
                    return 0;
                }
            } else {
                match conditional_node {
                    ConditionalNode::VElseIf { node, condition } => {
                        // Generate `: ` on newline
                        self.code_helper.newline(buf);
                        CodeHelper::colon(buf);

                        self.generate_if_node(buf, node, &condition);
                        curr_index = index;
                        nodes_generated += 1;
                        newlines_count += 1;
                    }

                    ConditionalNode::VElse(node) => {
                        // Generate v-else prefix block
                        self.code_helper.newline(buf);
                        CodeHelper::colon(buf);

                        // Compile a node and mark that we had a closing v-else
                        self.compile_node(buf, node, true);
                        had_v_else = true;
                        nodes_generated += 1;

                        // v-else should always be the last node in the conditional sequence
                        break;
                    }

                    _ => unreachable!("not reachable because v-if is always processed first"),
                }
            }

            // Exit condition: next index is not consecutive or `v-if` is a next conditional node
            if let Some((index, conditional_node)) = nodes.peek() {
                if *index - curr_index > 1 {
                    break;
                }
                if let ConditionalNode::VIf { .. } = conditional_node {
                    break;
                }
            }
        }

        // Cleanup
        if nodes_generated > 0 && !had_v_else {
            self.generate_closing_v_else(buf);
        }
        for _ in 0..newlines_count {
            self.code_helper.unindent();
        }

        nodes_generated
    }

    /// Generates a prefix block for `v-if` and `v-else-if`
    fn generate_if_node(&mut self, buf: &mut String, node: &Node, condition: &str) {
        // TODO use context-scope based compilation

        // Write condition
        buf.push_str(condition);

        // Write a question mark (`true` branch)
        self.code_helper.indent();
        self.code_helper.newline(buf);
        buf.push_str("? ");

        // Compile node itself
        self.compile_node(buf, node, true);
    }

    /// Generates a closing branch of an unclosed `v-if`, e.g. when only `v-if` is present
    fn generate_closing_v_else(&mut self, buf: &mut String) {
        self.code_helper.newline(buf);
        buf.push_str(": ");
        buf.push_str(self.get_and_add_import_str(VueImports::CreateCommentVNode));
        buf.push_str(r#"("v-if", true)"#);
    }
}

pub enum ConditionalNode<'a> {
    VIf {
        node: &'a Node<'a>,
        condition: &'a str,
    },
    VElseIf {
        node: &'a Node<'a>,
        condition: &'a str,
    },
    VElse(&'a Node<'a>),
}

/// Filters the nodes which have conditional directives in them (v-if, v-else and v-else-if)
/// Returns an iterator where each item is a pair of (usize, ConditionalNode).
/// First element of the pair is an index of the element in the original slice
pub fn filter_nodes_with_conditional_directives<'r>(
    nodes: &'r [Node<'r>],
) -> impl Iterator<Item = (usize, ConditionalNode<'r>)> {
    nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            Node::Element(element_node) => {
                let Some(ref directives) = element_node.starting_tag.directives else {
                    return None;
                };

                if let Some(v_if) = directives.v_if {
                    Some((
                        index,
                        ConditionalNode::VIf {
                            node,
                            condition: v_if,
                        },
                    ))
                } else if let Some(v_else_if) = directives.v_else_if {
                    Some((
                        index,
                        ConditionalNode::VElseIf {
                            node,
                            condition: v_else_if,
                        },
                    ))
                } else if let Some(_) = directives.v_else {
                    Some((index, ConditionalNode::VElse(node)))
                } else {
                    None
                }
            }
            _ => None,
        })
}
