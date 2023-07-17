use fervid_core::ConditionalNodeSequence;
use swc_core::{
    common::{Spanned, DUMMY_SP},
    ecma::ast::{CondExpr, Expr},
};

use crate::context::CodegenContext;

impl CodegenContext {
    pub fn generate_conditional_seq(&mut self, conditional_seq: &ConditionalNodeSequence) -> Expr {
        let mut conditional_exprs = Vec::new();

        // First, push the `if` node
        let if_conditional = &conditional_seq.if_node;
        let if_expr = &if_conditional.condition;
        let if_element_node = &if_conditional.node;
        // let _has_js = transform_scoped(&mut if_expr, &self.scope_helper, if_element_node.template_scope);
        conditional_exprs.push(Box::new(if_expr.to_owned()));
        conditional_exprs.push(Box::new(
            self.generate_element_or_component(if_element_node, false).0,
        ));

        // Then, push all the `else-if` nodes
        for else_if_conditional in conditional_seq.else_if_nodes.iter() {
            let else_if_expr = &else_if_conditional.condition;
            let else_if_node = &else_if_conditional.node;

            // let _has_js = transform_scoped(&mut else_if_expr, &self.scope_helper, else_if_node.template_scope);
            conditional_exprs.push(Box::new(else_if_expr.to_owned()));
            conditional_exprs.push(Box::new(
                self.generate_element_or_component(else_if_node, false).0,
            ));
        }

        // Push either `else` or a comment node
        let else_expr = if let Some(ref else_node) = conditional_seq.else_node {
            self.generate_element_or_component(else_node, false).0
        } else {
            self.generate_comment_vnode("v-if", DUMMY_SP)
        };
        conditional_exprs.push(Box::new(else_expr));

        // And lastly, fold the results in triplets from the back
        // (..., condition, pos_branch, neg_branch) -> (..., expr)
        while conditional_exprs.len() >= 3 {
            let Some(negative_branch) = conditional_exprs.pop() else { unreachable!() };
            let Some(positive_branch) = conditional_exprs.pop() else { unreachable!() };
            let Some(condition) = conditional_exprs.pop() else { unreachable!() };

            // Combine 3 expressions into one ternary
            let ternary_expr = Expr::Cond(CondExpr {
                span: condition.span(),
                test: condition,
                cons: positive_branch,
                alt: negative_branch,
            });

            // Push back for the next iteration
            conditional_exprs.push(Box::new(ternary_expr));
        }

        // Get the final result and return it
        assert!(conditional_exprs.len() == 1);
        let Some(resulting_expr) = conditional_exprs.pop() else {
            unreachable!()
        };

        // I don't like the idea of dereferencing a Box, but the signature requires it
        *resulting_expr
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{ElementNode, Node, StartingTag, Conditional};

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_v_if() {
        // <h1 v-if="foo || true">hello</h1>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo || true"),
                    node: ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1",
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![Node::Text("hello")],
                        template_scope: 0,
                    },
                }),
                else_if_nodes: vec![],
                else_node: None,
            },
            r#"_ctx.foo||true?_createElementVNode("h1",null,"hello"):_createCommentVNode("v-if")"#,
        )
    }

    #[test]
    fn it_generates_v_else() {
        // <h1 v-if="foo || true">hello</h1>
        // <h2 v-else>bye</h2>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo || true"),
                    node: ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1",
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![Node::Text("hello")],
                        template_scope: 0,
                    },
                }),
                else_if_nodes: vec![],
                else_node: Some(Box::new(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "h2",
                        attributes: vec![],
                        directives: None
                    },
                    children: vec![Node::Text("bye")],
                    template_scope: 0,
                })),
            },
            r#"_ctx.foo||true?_createElementVNode("h1",null,"hello"):_createElementVNode("h2",null,"bye")"#,
        )
    }

    #[test]
    fn it_generates_v_else_if() {
        // <h1 v-if="foo">hello</h1>
        // <h2 v-else-if="true">hi</h2>
        // <h3 v-else-if="undefined">bye</h2>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo || true"),
                    node: ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1",
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![Node::Text("hello")],
                        template_scope: 0,
                    },
                }),
                else_if_nodes: vec![
                    Conditional {
                        condition: *js("true"),
                        node: ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h2",
                                attributes: vec![],
                                directives: None
                            },
                            children: vec![Node::Text("hi")],
                            template_scope: 0,
                        },
                    },
                    Conditional {
                        condition: *js("undefined"),
                        node: ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h3",
                                attributes: vec![],
                                directives: None
                            },
                            children: vec![Node::Text("bye")],
                            template_scope: 0,
                        },
                    },
                ],
                else_node: None,
            },
            r#"_ctx.foo?_createElementVNode("h1",null,"hello"):true?_createElementVNode("h2",null,"hi"):undefined?_createElementVNode("h3",null,"bye"):_createCommentVNode("v-if")"#,
        )
    }

    #[test]
    fn it_generates_complex() {
        // <h1 v-if="foo">hello</h1>
        // <h2 v-else-if="true">hi</h2>
        // <h3 v-else-if="undefined">good morning</h2>
        // <h4 v-else>bye</h4>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo"),
                    node: ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1",
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![Node::Text("hello")],
                        template_scope: 0,
                    },
                }),
                else_if_nodes: vec![
                    Conditional {
                        condition: *js("true"),
                        node: ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h2",
                                attributes: vec![],
                                directives: None
                            },
                            children: vec![Node::Text("hi")],
                            template_scope: 0,
                        },
                    },
                    Conditional {
                        condition: *js("undefined"),
                        node: ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h3",
                                attributes: vec![],
                                directives: None
                            },
                            children: vec![Node::Text("good morning")],
                            template_scope: 0,
                        },
                    },
                ],
                else_node: Some(Box::new(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "h4",
                        attributes: vec![],
                        directives: None
                    },
                    children: vec![Node::Text("bye")],
                    template_scope: 0,
                })),
            },
            r#"_ctx.foo?_createElementVNode("h1",null,"hello"):true?_createElementVNode("h2",null,"hi"):undefined?_createElementVNode("h3",null,"good morning"):_createElementVNode("h4",null,"bye")"#,
        )
    }

    fn test_out(input: ConditionalNodeSequence, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_conditional_seq(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
