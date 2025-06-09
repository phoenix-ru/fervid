use fervid_core::{
    check_attribute_name, fervid_atom, AttributeOrBinding, ElementNode, IntoIdent, VueImports,
};
use swc_core::ecma::ast::{
    ArrayLit, CallExpr, Callee, Expr, ExprOrSpread, Lit, MemberExpr, MemberProp, ObjectLit, Str,
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the code for `<slot>` element.
    ///
    /// A typical code (stringified) has the following form:
    /// ```js
    /// renderSlot(_ctx.$slots, "slot-name", /*optional*/ { slot: attributes }, /*optional*/ [slot, children])
    /// ```
    pub fn generate_slot(&mut self, element_node: &ElementNode) -> Expr {
        let span = element_node.span;

        // The `name` attribute should NOT be generated,
        // therefore we split attributes generation to two slices, like so:
        // ---1--- "name" ---2---
        // This way we preserve the original order of attributes
        // and avoid sorting or allocating extra.
        let idx_of_name = element_node
            .starting_tag
            .attributes
            .iter()
            .position(|attr| check_attribute_name(attr, "name"));

        // Determine the args length (remember, we exclude `name` from attrs length)
        let has_children = element_node.children.len() > 0;
        let has_attributes =
            element_node.starting_tag.attributes.len() > idx_of_name.map_or(0, |_| 1);

        let render_slot_args_len = if has_children {
            4
        } else if has_attributes {
            3
        } else {
            2
        };

        let mut render_slot_args: Vec<ExprOrSpread> = Vec::with_capacity(render_slot_args_len);

        // First arg: `_ctx.$slots`
        render_slot_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Member(MemberExpr {
                span,
                obj: Box::new(Expr::Ident(fervid_atom!("_ctx").into_ident_spanned(span))),
                prop: MemberProp::Ident(fervid_atom!("$slots").into_ident_spanned(span).into()),
            })),
        });

        // Second arg: slot name (`name="foo"`), slot expression (`:name="foo"`) or "default"
        let name_expr = if let Some(idx) = idx_of_name {
            let name_attr = &element_node.starting_tag.attributes[idx];

            match name_attr {
                AttributeOrBinding::RegularAttribute { value, .. } => Expr::Lit(Lit::Str(Str {
                    span,
                    value: value.to_owned(),
                    raw: None,
                })),
                AttributeOrBinding::VBind(v_bind) => (*v_bind.value).to_owned(),

                _ => unreachable!(),
            }
        } else {
            Expr::Lit(Lit::Str(Str {
                span,
                value: fervid_atom!("default"),
                raw: None,
            }))
        };

        render_slot_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(name_expr),
        });

        // Third arg (optional): attributes
        if has_attributes {
            let mut attrs_obj = ObjectLit {
                span,
                props: Vec::with_capacity(element_node.starting_tag.attributes.len()),
            };

            match idx_of_name {
                // Split attributes to two slices if we have a `name`
                Some(idx) => {
                    let attrs_slice1 = &element_node.starting_tag.attributes[..idx];
                    let attrs_slice2 = &element_node.starting_tag.attributes[(idx + 1)..];

                    // TODO Consider attr hints?
                    self.generate_attributes(attrs_slice1, &mut attrs_obj.props);
                    self.generate_attributes(attrs_slice2, &mut attrs_obj.props);
                }

                // TODO Consider attr hints?
                None => {
                    self.generate_attributes(
                        &element_node.starting_tag.attributes,
                        &mut attrs_obj.props,
                    );
                }
            }

            render_slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(attrs_obj)),
            });
        } else if has_children {
            // Pushes `{}` as third argument
            render_slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(ObjectLit {
                    span,
                    props: vec![],
                })),
            })
        }

        // Fourth arg (optional): children
        if has_children {
            let slot_children = self
                .generate_element_children(element_node, false)
                .0
                .into_iter()
                .map(|expr| {
                    Some(ExprOrSpread {
                        spread: None,
                        expr: Box::new(expr),
                    })
                })
                .collect();

            render_slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Array(ArrayLit {
                    span,
                    elems: slot_children,
                })),
            });
        }

        // `renderSlot(_ctx.$slots, "slot-name", { slot: attributes }, [slot, children])`
        Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::RenderSlot)
                    .into_ident_spanned(span),
            ))),
            args: render_slot_args,
            type_args: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{BuiltinType, ElementKind, Node, StartingTag};
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{regular_attribute, v_bind_attribute};

    use super::*;

    macro_rules! slot {
        ($attributes: expr, $children: expr) => {
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Slot),
                starting_tag: StartingTag {
                    tag_name: "slot".into(),
                    attributes: $attributes,
                    directives: None,
                },
                children: $children,
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            }
        };
    }

    #[test]
    fn it_generates_default_slot() {
        // <slot />
        test_out(
            slot!(vec![], vec![]),
            r#"_renderSlot(_ctx.$slots,"default")"#,
        );

        // <slot name="default" />
        test_out(
            slot!(vec![regular_attribute("name", "default")], vec![]),
            r#"_renderSlot(_ctx.$slots,"default")"#,
        );
    }

    #[test]
    fn it_generates_named_slot() {
        // <slot name="test-slot" />
        test_out(
            slot!(vec![regular_attribute("name", "test-slot")], vec![]),
            r#"_renderSlot(_ctx.$slots,"test-slot")"#,
        );
    }

    #[test]
    fn it_generates_dynamically_named_slot() {
        // <slot :name="slot + name" />
        test_out(
            slot!(vec![v_bind_attribute("name", "slot + name")], vec![]),
            r#"_renderSlot(_ctx.$slots,slot+name)"#,
        );
    }

    #[test]
    fn it_generates_attrs() {
        // <slot foo="bar" :baz="qux" />
        test_out(
            slot!(
                vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                vec![]
            ),
            r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:qux})"#,
        );

        // <slot name="default" foo="bar" :baz="qux" />
        test_out(
            slot!(
                vec![
                    regular_attribute("name", "default"),
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                vec![]
            ),
            r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:qux})"#,
        );

        // <slot foo="bar" name="default" :baz="qux" />
        test_out(
            slot!(
                vec![
                    regular_attribute("foo", "bar"),
                    regular_attribute("name", "default"),
                    v_bind_attribute("baz", "qux"),
                ],
                vec![]
            ),
            r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:qux})"#,
        );

        // <slot foo="bar" :baz="qux" name="default" />
        test_out(
            slot!(
                vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                    regular_attribute("name", "default"),
                ],
                vec![]
            ),
            r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:qux})"#,
        );
    }

    #[test]
    fn it_generates_children() {
        // <slot>
        //   <div>Placeholder</div>
        //   <foo-component />
        // </slot>
        test_out(
            slot!(
                vec![],
                vec![
                    Node::Element(ElementNode {
                        kind: ElementKind::Element,
                        starting_tag: StartingTag {
                            tag_name: "div".into(),
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![Node::Text("Placeholder".into(), DUMMY_SP)],
                        template_scope: 0,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    }),
                    Node::Element(ElementNode {
                        kind: ElementKind::Component,
                        starting_tag: StartingTag {
                            tag_name: "foo-component".into(),
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![],
                        template_scope: 0,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    })
                ]
            ),
            r#"_renderSlot(_ctx.$slots,"default",{},[_createElementVNode("div",null,"Placeholder"),_createVNode(_component_foo_component)])"#,
        );
    }

    #[test]
    fn it_generates_attrs_and_children() {
        // <slot name="test-slot" foo="bar" :baz="qux">
        //   <div>Placeholder</div>
        //   <foo-component />
        // </slot>
        test_out(
            slot!(
                vec![
                    regular_attribute("name", "test-slot"),
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                vec![
                    Node::Element(ElementNode {
                        kind: ElementKind::Element,
                        starting_tag: StartingTag {
                            tag_name: "div".into(),
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![Node::Text("Placeholder".into(), DUMMY_SP)],
                        template_scope: 0,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    }),
                    Node::Element(ElementNode {
                        kind: ElementKind::Component,
                        starting_tag: StartingTag {
                            tag_name: "foo-component".into(),
                            attributes: vec![],
                            directives: None
                        },
                        children: vec![],
                        template_scope: 0,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    })
                ]
            ),
            r#"_renderSlot(_ctx.$slots,"test-slot",{foo:"bar",baz:qux},[_createElementVNode("div",null,"Placeholder"),_createVNode(_component_foo_component)])"#,
        );
    }

    fn test_out(input: ElementNode, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_slot(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
