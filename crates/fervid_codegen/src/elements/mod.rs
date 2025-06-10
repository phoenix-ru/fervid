use fervid_core::{
    AttributeOrBinding, ElementNode, IntoIdent, StartingTag, StrOrExpr, VBindDirective, VueImports,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{
            ArrayLit, CallExpr, Callee, Expr, ExprOrSpread, Lit, Null, Number, ObjectLit,
            PropOrSpread, Str,
        },
        atoms::JsWord,
    },
};

use crate::{context::CodegenContext, control_flow::SlottedIterator};

impl CodegenContext {
    pub fn generate_element_vnode(
        &mut self,
        element_node: &ElementNode,
        wrap_in_block: bool,
    ) -> Expr {
        let span = DUMMY_SP;
        let starting_tag = &element_node.starting_tag;

        // Generate attributes
        let attributes = self.generate_element_attributes(element_node);
        let attributes_expr = if !attributes.is_empty() {
            Some(Expr::Object(ObjectLit {
                span,
                props: attributes,
            }))
        } else {
            None
        };

        // There is a special case here: `<template>` with `v-if`/`v-else-if`/`v-else`/`v-for`
        let should_generate_fragment_instead = (wrap_in_block
            && element_node.starting_tag.tag_name == "template")
            || self.should_generate_fragment(element_node);

        // Generate children
        // Inlining is forbidden if we changed from `<template>` to `Fragment`
        let (mut children, was_inlined) =
            self.generate_element_children(element_node, !should_generate_fragment_instead);

        // Wire the things together. `createElementVNode` args:
        // 1st - element name or Fragment;
        // 2nd (optional) - element attributes & directives object;
        // 3rd (optional) - element children;
        // 4th (optional) - element patch flag;
        // 5th (optional) - props array (for PROPS patch flag).
        let expected_element_args_count = if !element_node.patch_hints.props.is_empty() {
            5
        } else if !element_node.patch_hints.flags.is_empty() {
            4
        } else if !children.is_empty() {
            3
        } else if attributes_expr.is_some() {
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
            Expr::Ident(
                self.get_and_add_import_ident(VueImports::Fragment)
                    .into_ident_spanned(span),
            )
        } else {
            Expr::Lit(Lit::Str(Str {
                span,
                value: starting_tag.tag_name.to_owned(),
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
            } else if !children.is_empty() {
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
            let patch_flag_value = element_node.patch_hints.flags.bits();

            create_element_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: patch_flag_value.into(),
                    raw: None,
                }))),
            });

            if !element_node.patch_hints.props.is_empty() {
                let props_array = element_node
                    .patch_hints
                    .props
                    .iter()
                    .map(|prop| {
                        Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Lit(Lit::Str(Str {
                                span: DUMMY_SP,
                                value: prop.to_owned(),
                                raw: None,
                            }))),
                        })
                    })
                    .collect();

                create_element_args.push(ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Array(ArrayLit {
                        span: DUMMY_SP,
                        elems: props_array,
                    })),
                });
            }
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
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                create_element_fn_ident.into_ident_spanned(span),
            ))),
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

        // Process directives
        create_element_expr = self.generate_element_directives(create_element_expr, element_node);

        create_element_expr
    }

    fn generate_element_attributes(&mut self, element_node: &ElementNode) -> Vec<PropOrSpread> {
        let mut result_props = Vec::new();

        self.generate_attributes(&element_node.starting_tag.attributes, &mut result_props);

        // Directives
        if let Some(ref directives) = element_node.starting_tag.directives {
            for v_model in directives.v_model.iter() {
                self.generate_v_model_for_element(v_model, &mut result_props);
            }

            if let Some(ref v_text) = directives.v_text {
                result_props.push(self.generate_v_text(v_text));
            }

            if let Some(ref v_html) = directives.v_html {
                result_props.push(self.generate_v_html(v_html));
            }
        }

        result_props
    }

    pub(crate) fn generate_element_children(
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
                slotted_iterator.advance();
            }

            slotted_iterator.toggle_mode();
        }

        (out, was_inlined)
    }

    fn generate_element_directives(
        &mut self,
        create_element_expr: Expr,
        element_node: &ElementNode,
    ) -> Expr {
        // Guard because we need the whole `ElementNode`, not just `VueDirectives`
        let Some(ref directives) = element_node.starting_tag.directives else {
            return create_element_expr;
        };

        // Output array for `withDirectives` call.
        // If this stays empty at the end, no processing to `create_element_expr` would be done
        let mut out: Vec<Option<ExprOrSpread>> = Vec::new();

        // Element `v-model` needs a special processing compared to a component one
        if !directives.v_model.is_empty() {
            let span = DUMMY_SP; // TODO Span
            let v_model_identifier = Expr::Ident(
                self.get_element_vmodel_directive_name(&element_node.starting_tag)
                    .into_ident_spanned(span),
            );

            for v_model in directives.v_model.iter() {
                out.push(Some(ExprOrSpread {
                    spread: None,
                    expr: Box::new(self.generate_directive_from_parts(
                        v_model_identifier.to_owned(),
                        Some(&v_model.value),
                        v_model.argument.as_ref(),
                        &v_model.modifiers,
                        DUMMY_SP,
                    )),
                }));
            }
        }

        self.generate_directives_to_array(directives, &mut out);
        self.maybe_generate_with_directives(create_element_expr, out)
    }

    fn get_element_vmodel_directive_name(&mut self, starting_tag: &StartingTag) -> JsWord {
        // These cases need special handling of v-model
        // input type=* -> vModelText
        // input type="radio" -> vModelRadio
        // input type="checkbox" -> vModelCheckbox
        // input :type=* -> vModelDynamic
        // select -> vModelSelect
        // textarea -> vModelText
        match starting_tag.tag_name.as_ref() {
            "input" => {
                // Find `type` attribute
                for attr in starting_tag.attributes.iter() {
                    match attr {
                        // type="smth"
                        AttributeOrBinding::RegularAttribute { name, value, .. }
                            if name == "type" =>
                        {
                            match value.as_ref() {
                                "checkbox" => {
                                    return self
                                        .get_and_add_import_ident(VueImports::VModelCheckbox)
                                }
                                "radio" => {
                                    return self.get_and_add_import_ident(VueImports::VModelRadio)
                                }
                                _ => return self.get_and_add_import_ident(VueImports::VModelText),
                            }
                        }

                        // :type="smth"
                        AttributeOrBinding::VBind(VBindDirective {
                            argument: Some(StrOrExpr::Str(s)),
                            ..
                        }) if s == "type" => {
                            return self.get_and_add_import_ident(VueImports::VModelDynamic)
                        }

                        _ => continue,
                    }
                }

                self.get_and_add_import_ident(VueImports::VModelText)
            }

            "select" => self.get_and_add_import_ident(VueImports::VModelSelect),

            // TODO Clean up such usage (except "textarea")? Or just use `VModelText`?
            _ => self.get_and_add_import_ident(VueImports::VModelText),
        }
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{ElementKind, Interpolation, Node, StartingTag};

    use super::*;
    use crate::test_utils::{js, regular_attribute, v_bind_attribute, v_on_attribute};

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
                    tag_name: "div".into(),
                    attributes: vec![
                        regular_attribute("foo", "bar"),
                        v_bind_attribute("baz", "qux"),
                        v_bind_attribute("readonly", "true"),
                        v_on_attribute("onClick", "handleClick"),
                    ],
                    directives: None,
                },
                children: vec![Node::Text("hello from div".into(), DUMMY_SP)],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createElementVNode("div",{foo:"bar",baz:qux,readonly:true,onClick:handleClick},"hello from div")"#,
            false,
        )
    }

    #[test]
    fn it_generates_attrless() {
        // <div>hello from div</div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![Node::Text("hello from div".into(), DUMMY_SP)],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
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
                    tag_name: "div".into(),
                    attributes: vec![
                        regular_attribute("foo", "bar"),
                        v_bind_attribute("some-baz", "qux"),
                    ],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createElementVNode("div",{foo:"bar","some-baz":qux})"#,
            false,
        );

        // <div foo="bar" />
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div".into(),
                    attributes: vec![
                        regular_attribute("foo", "bar"),
                        v_bind_attribute("some-baz", "qux"),
                    ],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createElementVNode("div",{foo:"bar","some-baz":qux})"#,
            false,
        )
    }

    #[test]
    fn it_generates_text_nodes_concatenation() {
        // <div>hello from div {{ true }} bye!</div>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Text("hello from div ".into(), DUMMY_SP),
                    Node::Interpolation(Interpolation {
                        value: js("true"),
                        template_scope: 0,
                        patch_flag: false,
                        span: DUMMY_SP,
                    }),
                    Node::Text(" bye!".into(), DUMMY_SP),
                ],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
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
                    tag_name: "div".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Text("hello from div ".into(), DUMMY_SP),
                    Node::Interpolation(Interpolation {
                        value: js("true"),
                        template_scope: 0,
                        patch_flag: false,
                        span: DUMMY_SP,
                    }),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "span".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("bye!".into(), DUMMY_SP)],
                        template_scope: 0,
                        kind: ElementKind::Element,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createElementVNode("div",null,[_createTextVNode("hello from div "+_toDisplayString(true)),_createElementVNode("span",null,"bye!")])"#,
            false,
        )
    }

    fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_element_vnode(&input, wrap_in_block);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
