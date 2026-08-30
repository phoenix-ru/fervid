use fervid_core::{
    AttributeOrBinding, ElementNode, IntoIdent, VBindDirective, VueImports, check_attribute_name,
    fervid_atom,
};
use swc_core::{
    common::Span,
    ecma::ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread, Lit,
        MemberExpr, MemberProp, ObjectLit, Str,
    },
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
        let has_children = !element_node.children.is_empty();
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

        // Third arg (optional): slot props
        if let Some(slot_props) = self.generate_slot_props(element_node, idx_of_name, span) {
            render_slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(slot_props),
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

        // Fourth arg (optional): slot children (fallback)
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

            // () => [child1, child2]
            let fallback = Box::new(Expr::Arrow(ArrowExpr {
                span,
                ctxt: Default::default(),
                params: vec![],
                body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Array(ArrayLit {
                    span,
                    elems: slot_children,
                })))),
                is_async: false,
                is_generator: false,
                type_params: None,
                return_type: None,
            }));

            render_slot_args.push(ExprOrSpread {
                spread: None,
                expr: fallback,
            });
        }

        // `renderSlot(_ctx.$slots, "slot-name", { slot: attributes }, () => [slot, children])`
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

    fn generate_slot_props(
        &mut self,
        element_node: &ElementNode,
        idx_of_name: Option<usize>,
        span: Span,
    ) -> Option<Expr> {
        let max_len = element_node.starting_tag.attributes.len();
        let mut merge_args = Vec::with_capacity(max_len);
        let mut segment = Vec::with_capacity(max_len);
        let mut has_argumentless_v_bind = false;

        for (index, attr) in element_node.starting_tag.attributes.iter().enumerate() {
            // `name` attribute itself shouldn't become a slot prop
            if Some(index) == idx_of_name {
                continue;
            }

            if let AttributeOrBinding::VBind(VBindDirective {
                argument: None,
                value,
                ..
            }) = attr
            {
                self.push_slot_props_segment(&segment, &mut merge_args, span);
                segment.clear();
                has_argumentless_v_bind = true;

                // Value was already transformed
                merge_args.push(*value.to_owned());
            } else {
                // TODO Avoid unnecessary cloning when re-writing this function
                segment.push(attr.clone());
            }
        }

        self.push_slot_props_segment(&segment, &mut merge_args, span);

        if merge_args.len() <= 1 {
            let value = merge_args.pop()?;

            // `<slot v-bind="obj" />` needs `normalizeProps(guardReactiveProps(obj))`
            if has_argumentless_v_bind {
                // TODO: scrap this and do inside transform instead
            }

            return Some(value);
        }

        Some(Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::MergeProps)
                    .into_ident(),
            ))),
            args: merge_args
                .into_iter()
                .map(|expr| ExprOrSpread {
                    spread: None,
                    expr: Box::new(expr),
                })
                .collect(),
            type_args: None,
        }))
    }

    // TODO: Re-implement the proper slot props codegen
    // using codegenNode when ready. It should be better optimized to avoid unnecessary cloning.
    fn push_slot_props_segment(
        &mut self,
        attributes: &[AttributeOrBinding],
        merge_args: &mut Vec<Expr>,
        span: Span,
    ) {
        if attributes.is_empty() {
            return;
        }

        let mut props = Vec::new();
        self.generate_attributes(attributes, &mut props);

        if !props.is_empty() {
            merge_args.push(Expr::Object(ObjectLit { span, props }));
        }
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{BuiltinType, ElementKind, Node, StartingTag};
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{js, regular_attribute, v_bind_attribute};

    use super::*;

    macro_rules! slot {
        ($attributes: expr, $children: expr) => {
            ElementNode::new_with_children_and_type(
                StartingTag {
                    tag_name: "slot".into(),
                    attributes: $attributes,
                    directives: None,
                },
                $children,
                ElementKind::Builtin(BuiltinType::Slot),
            )
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
                    Node::Element(ElementNode::new_with_children(
                        StartingTag {
                            tag_name: "div".into(),
                            attributes: vec![],
                            directives: None
                        },
                        vec![Node::Text("Placeholder".into(), DUMMY_SP)],
                    )),
                    Node::Element(ElementNode::new_with_children_and_type(
                        StartingTag {
                            tag_name: "foo-component".into(),
                            attributes: vec![],
                            directives: None
                        },
                        vec![],
                        ElementKind::Component,
                    ))
                ]
            ),
            r#"_renderSlot(_ctx.$slots,"default",{},()=>[_createElementVNode("div",null,"Placeholder"),_createVNode(_component_foo_component)])"#,
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
                    Node::Element(ElementNode::new_with_children(
                        StartingTag {
                            tag_name: "div".into(),
                            attributes: vec![],
                            directives: None
                        },
                        vec![Node::Text("Placeholder".into(), DUMMY_SP)],
                    )),
                    Node::Element(ElementNode::new_with_children_and_type(
                        StartingTag {
                            tag_name: "foo-component".into(),
                            attributes: vec![],
                            directives: None
                        },
                        vec![],
                        ElementKind::Component
                    ))
                ]
            ),
            r#"_renderSlot(_ctx.$slots,"test-slot",{foo:"bar",baz:qux},()=>[_createElementVNode("div",null,"Placeholder"),_createVNode(_component_foo_component)])"#,
        );
    }

    #[test]
    fn it_normalizes_argumentless_v_bind() {
        // <slot v-bind="obj" />
        test_out(
            slot!(
                vec![AttributeOrBinding::VBind(VBindDirective {
                    argument: None,
                    value: js("obj"),
                    is_camel: false,
                    is_prop: false,
                    is_attr: false,
                    span: DUMMY_SP,
                })],
                vec![]
            ),
            "_renderSlot(_ctx.$slots,\"default\",obj)",
        );
    }

    #[test]
    fn it_merges_argumentless_v_bind_in_source_order() {
        // <slot foo="before" v-bind="obj" bar="after" />
        test_out(
            slot!(
                vec![
                    regular_attribute("foo", "before"),
                    AttributeOrBinding::VBind(VBindDirective {
                        argument: None,
                        value: js("obj"),
                        is_camel: false,
                        is_prop: false,
                        is_attr: false,
                        span: DUMMY_SP,
                    }),
                    regular_attribute("bar", "after"),
                ],
                vec![]
            ),
            "_renderSlot(_ctx.$slots,\"default\",_mergeProps({foo:\"before\"},obj,{bar:\"after\"}))",
        );
    }

    fn test_out(input: ElementNode, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_slot(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
