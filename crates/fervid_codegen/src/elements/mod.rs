use fervid_core::ElementNode;
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{
            ArrayLit, CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, Null, Number, ObjectLit,
            PropOrSpread, Str,
        },
        atoms::JsWord,
    },
};

use crate::{
    attributes::DirectivesToProcess, context::CodegenContext, control_flow::SlottedIterator,
    imports::VueImports,
};

impl CodegenContext {
    pub fn generate_element_vnode(
        &mut self,
        element_node: &ElementNode,
        wrap_in_block: bool,
    ) -> Expr {
        // TODO
        let needs_patch_flags = false;
        let span = DUMMY_SP;
        let starting_tag = &element_node.starting_tag;

        // Generate attributes
        let (attributes, remaining_directives) = self.generate_element_attributes(element_node);
        let attributes_expr = if attributes.len() != 0 {
            Some(Expr::Object(ObjectLit {
                span,
                props: attributes,
            }))
        } else {
            None
        };

        // There is a special case here: `<template>` with `v-if`/`v-else-if`/`v-else`/`v-for`
        let should_generate_fragment_instead = self.should_generate_fragment(element_node);

        // Generate children
        // Inlining is forbidden if we changed from `<template>` to `Fragment`
        let (mut children, was_inlined) =
            self.generate_element_children(element_node, !should_generate_fragment_instead);

        // Wire the things together. `createElementVNode` args:
        // 1st - element name or Fragment;
        // 2nd (optional) - element attributes & directives object;
        // 3rd (optional) - element children;
        // 4th (optional) - element patch flag.
        let expected_element_args_count = if needs_patch_flags {
            4
        } else if children.len() != 0 {
            3
        } else if let Some(_) = attributes_expr {
            2
        } else {
            1
        };

        /// Produces a `null` expression
        macro_rules! null {
            () => {
                Box::new(Expr::Lit(Lit::Null(Null { span })))
            };
        }

        // Arguments for function call
        let mut create_element_args = Vec::with_capacity(expected_element_args_count);

        // Arg 1: element name. Either a stringified name or Fragment
        let element_name_expr = if should_generate_fragment_instead {
            Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::Fragment),
                optional: false,
            })
        } else {
            Expr::Lit(Lit::Str(Str {
                span,
                value: JsWord::from(starting_tag.tag_name),
                raw: None,
            }))
        };
        create_element_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(element_name_expr),
        });

        // Arg 2 (optional): component attributes expression (default to null)
        if expected_element_args_count >= 2 {
            let expr_to_push = if let Some(attributes_expr) = attributes_expr {
                Box::new(attributes_expr)
            } else {
                null!()
            };
            create_element_args.push(ExprOrSpread {
                spread: None,
                expr: expr_to_push,
            });
        }

        // Arg 3 (optional): component children expression (default to null).
        // This may be a text concatenation, an array of child nodes, or `null`.
        if expected_element_args_count >= 3 {
            let expr_to_push = if was_inlined && children.len() == 1 {
                // When all children were inlined into one Expr, use this expr
                let Some(child) = children.pop() else {
                    unreachable!()
                };

                Box::new(child)
            } else if children.len() != 0 {
                // [child1, child2, child3]
                let children: Vec<Option<ExprOrSpread>> = children
                    .into_iter()
                    .map(|child| {
                        Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(child),
                        })
                    })
                    .collect();

                Box::new(Expr::Array(ArrayLit {
                    span,
                    elems: children,
                }))
            } else {
                null!()
            };

            create_element_args.push(ExprOrSpread {
                spread: None,
                expr: expr_to_push,
            })
        }

        // Arg 4 (optional): patch flags (default to nothing)
        if expected_element_args_count >= 4 {
            // TODO Actual patch flag value
            create_element_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: 512.0, // TODO
                    raw: None,
                }))),
            })
        }

        // When wrapping in block, `createElementBlock` is used, otherwise `createElementVNode`
        let create_element_fn_ident = self.get_and_add_import_ident(if wrap_in_block {
            VueImports::CreateElementBlock
        } else {
            VueImports::CreateElementVNode
        });

        // `createElementVNode("element-name", {element:attrs}, [element, children], PATCH_FLAGS)`
        let create_element_fn_call = Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: create_element_fn_ident,
                optional: false,
            }))),
            args: create_element_args,
            type_args: None,
        });

        // When wrapping in block, we also need `openBlock()`
        let mut create_element_expr = if wrap_in_block {
            // (openBlock(), createElementBlock("element-name", {element:attrs}, [element, children], PATCH_FLAGS))
            self.wrap_in_open_block(create_element_fn_call, span)
        } else {
            // Just `createElementVNode` call
            create_element_fn_call
        };

        // Process remaining directives
        if remaining_directives.len() != 0 {
            self.generate_remaining_element_directives(
                &mut create_element_expr,
                &remaining_directives,
            );
        }

        create_element_expr
    }

    fn generate_element_attributes<'e>(
        &mut self,
        element_node: &'e ElementNode,
    ) -> (Vec<PropOrSpread>, DirectivesToProcess<'e>) {
        let mut result_props = Vec::new();
        let mut remaining_directives = DirectivesToProcess::new();

        self.generate_attributes(
            &element_node.starting_tag.attributes,
            &mut result_props,
            &mut remaining_directives,
            element_node.template_scope,
        );

        (result_props, remaining_directives)
    }

    fn generate_element_children(
        &mut self,
        element_node: &ElementNode,
        allow_inlining: bool,
    ) -> (Vec<Expr>, bool) {
        let mut was_inlined = true;
        let total_children = element_node.children.len();
        if total_children == 0 {
            return (Vec::new(), !was_inlined);
        }

        let mut out: Vec<Expr> = Vec::with_capacity(total_children);

        // `SlottedIterator` will iterate over sequences of default or named slots,
        // and it will stop yielding elements unless [`SlottedIterator::toggle_mode`] is called.
        let mut slotted_iterator = SlottedIterator::new(&element_node.children);

        while slotted_iterator.has_more() {
            if slotted_iterator.is_default_slot_mode() {
                was_inlined &= self.generate_node_sequence(
                    &mut slotted_iterator,
                    &mut out,
                    total_children,
                    allow_inlining,
                );
            } else {
                // Ignore named slots in the elements.
                // These should be reported in the analyzer.
                was_inlined = false;
            }

            slotted_iterator.toggle_mode();
        }

        (out, was_inlined)
    }

    fn generate_remaining_element_directives(
        &mut self,
        create_element_expr: &mut Expr,
        remaining_directives: &DirectivesToProcess,
    ) {
        // TODO for v-models in elements `withDirectives` needs a bit more information
        self.generate_remaining_directives(create_element_expr, remaining_directives)
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{HtmlAttribute, Node, StartingTag, VBindDirective, VDirective, VOnDirective};

    use super::*;

    #[test]
    fn it_generates_basic_usage() {
        // <div
        //   foo="bar"
        //   :baz="qux"
        //   :readonly="true"
        //   @click="handleClick"
        // >hello from div</div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![
                        HtmlAttribute::Regular {
                            name: "foo",
                            value: "bar",
                        },
                        HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                            argument: Some("baz"),
                            value: "qux",
                            is_dynamic_attr: false,
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        })),
                        HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                            argument: Some("readonly"),
                            value: "true",
                            is_dynamic_attr: false,
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        })),
                        HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                            event: Some("click"),
                            handler: Some("handleClick"),
                            is_dynamic_event: false,
                            modifiers: vec![],
                        })),
                    ],
                },
                children: vec![Node::TextNode("hello from div")],
                template_scope: 0,
            },
            r#"_createElementVNode("div",{foo:"bar",baz:_ctx.qux,readonly:true,onClick:_ctx.handleClick},"hello from div")"#,
            false,
        )
    }

    #[test]
    fn it_generates_attrless() {
        // <div>hello from div</div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![],
                },
                children: vec![Node::TextNode("hello from div")],
                template_scope: 0,
            },
            r#"_createElementVNode("div",null,"hello from div")"#,
            false,
        )
    }

    #[test]
    fn it_generates_childless() {
        // <div foo="bar"></div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![
                        HtmlAttribute::Regular {
                            name: "foo",
                            value: "bar",
                        },
                        HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                            argument: Some("some-baz"),
                            value: "qux",
                            is_dynamic_attr: false,
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        })),
                    ],
                },
                children: vec![],
                template_scope: 0,
            },
            r#"_createElementVNode("div",{foo:"bar","some-baz":_ctx.qux})"#,
            false,
        );

        // <div foo="bar" />
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![
                        HtmlAttribute::Regular {
                            name: "foo",
                            value: "bar",
                        },
                        HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                            argument: Some("some-baz"),
                            value: "qux",
                            is_dynamic_attr: false,
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        })),
                    ],
                },
                children: vec![],
                template_scope: 0,
            },
            r#"_createElementVNode("div",{foo:"bar","some-baz":_ctx.qux})"#,
            false,
        )
    }

    #[test]
    fn it_generates_text_nodes_concatenation() {
        // <div>hello from div {{ true }} bye!</div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![],
                },
                children: vec![
                    Node::TextNode("hello from div "),
                    Node::DynamicExpression {
                        value: "true",
                        template_scope: 0,
                    },
                    Node::TextNode(" bye!"),
                ],
                template_scope: 0,
            },
            r#"_createElementVNode("div",null,"hello from div "+_toDisplayString(true)+" bye!")"#,
            false,
        )
    }

    #[test]
    fn it_generates_children() {
        // <div>hello from div {{ true }}<span>bye!</span></div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div",
                    attributes: vec![],
                },
                children: vec![
                    Node::TextNode("hello from div "),
                    Node::DynamicExpression {
                        value: "true",
                        template_scope: 0,
                    },
                    Node::ElementNode(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "span",
                            attributes: vec![],
                        },
                        children: vec![Node::TextNode("bye!")],
                        template_scope: 0,
                    }),
                ],
                template_scope: 0,
            },
            r#"_createElementVNode("div",null,[_createTextVNode("hello from div "+_toDisplayString(true),1),_createElementVNode("span",null,"bye!")])"#,
            false,
        )
    }

    fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_element_vnode(&input, wrap_in_block);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
