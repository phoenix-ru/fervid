use fervid_core::{
    BuiltinType, DynamicSlot, DynamicSlotBuild, ElementNode, IntoIdent, Node, SlotBuild, SlotFlag,
    SlotSource, StrOrExpr, VNodeCallTag, VNodeChildren, VueImports, fervid_atom,
    str_or_expr_to_propname,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrayLit, ArrowExpr, ArrowFunctionBody, CallExpr, Callee, CondExpr, Expr, IdentName,
        KeyValueProp, Lit, Number, ObjectLit, ParenExpr, Pat, Prop, PropName, PropOrSpread,
    },
};

use crate::{
    CodegenContext,
    control_flow::SlottedIterator,
    utils::{expr_arg, null_expr},
};

impl CodegenContext {
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

    pub(crate) fn generate_vnode_children(
        &mut self,
        element_node: &ElementNode,
        children: &VNodeChildren,
        vnode_tag: &VNodeCallTag,
    ) -> Expr {
        match children {
            VNodeChildren::UseElementChildren => {
                let allow_inlining = !matches!(
                    vnode_tag,
                    VNodeCallTag::Fragment
                        | VNodeCallTag::Builtin(BuiltinType::Teleport | BuiltinType::KeepAlive)
                );
                let (mut children, was_inlined) =
                    self.generate_element_children(element_node, allow_inlining);
                if was_inlined && children.len() == 1 {
                    children.pop().unwrap_or_else(null_expr)
                } else {
                    Expr::Array(ArrayLit {
                        span: DUMMY_SP,
                        elems: children
                            .into_iter()
                            .map(|child| Some(expr_arg(child)))
                            .collect(),
                    })
                }
            }
            VNodeChildren::UseFirstChildTextNode => element_node
                .children
                .first()
                .map(|child| self.generate_node(child, false))
                .unwrap_or_else(null_expr),
            VNodeChildren::Slots(slots) => {
                let mut props = slots
                    .slots
                    .iter()
                    .map(|slot| self.generate_slot_property(element_node, slot))
                    .collect::<Vec<_>>();

                props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(IdentName {
                        span: DUMMY_SP,
                        sym: "_".into(),
                    }),
                    value: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value: (slots.slot_flag as u8).into(),
                        raw: Some(match slots.slot_flag {
                            SlotFlag::Stable => "1 /* STABLE */".into(),
                            SlotFlag::Dynamic => "2 /* DYNAMIC */".into(),
                            SlotFlag::Forwarded => "3 /* FORWARDED */".into(),
                        }),
                    }))),
                }))));

                let static_slots = Expr::Object(ObjectLit {
                    span: DUMMY_SP,
                    props,
                });

                if slots.dynamic_slots.is_empty() {
                    static_slots
                } else {
                    let dynamic_slots = slots
                        .dynamic_slots
                        .iter()
                        .map(|slot| Some(expr_arg(self.generate_dynamic_slot(element_node, slot))))
                        .collect();

                    Expr::Call(CallExpr {
                        span: DUMMY_SP,
                        ctxt: Default::default(),
                        callee: Callee::Expr(Box::new(Expr::Ident(
                            self.get_and_add_import_ident(VueImports::CreateSlots)
                                .into_ident(),
                        ))),
                        args: vec![
                            expr_arg(static_slots),
                            expr_arg(Expr::Array(ArrayLit {
                                span: DUMMY_SP,
                                elems: dynamic_slots,
                            })),
                        ],
                        type_args: None,
                    })
                }
            }
        }
    }

    fn generate_slot_property(
        &mut self,
        element_node: &ElementNode,
        slot: &SlotBuild,
    ) -> PropOrSpread {
        // "slot-name": withCtx((props) => [child1, child2])
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_or_expr_to_propname(slot.name.to_owned(), DUMMY_SP),
            value: self.generate_slot_function(element_node, slot.props.as_deref(), &slot.source),
        })))
    }

    fn generate_slot_function(
        &mut self,
        element_node: &ElementNode,
        props: Option<&Pat>,
        source: &SlotSource,
    ) -> Box<Expr> {
        let slot_children = get_slot_source_nodes(element_node, source);
        let total_children = slot_children.len();

        let mut generated_children = Vec::new();
        self.generate_node_sequence(
            &mut slot_children.into_iter(),
            &mut generated_children,
            total_children,
            false,
        );

        let body = Expr::Array(ArrayLit {
            span: DUMMY_SP,
            elems: generated_children
                .into_iter()
                .map(|child| Some(expr_arg(child)))
                .collect(),
        });

        let arrow = Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            params: props.cloned().into_iter().collect(),
            body: Box::new(ArrowFunctionBody::Expr(Box::new(body))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        });

        Box::new(Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::WithCtx)
                    .into_ident(),
            ))),
            args: vec![expr_arg(arrow)],
            type_args: None,
        }))
    }

    fn generate_dynamic_slot(&mut self, element_node: &ElementNode, slot: &DynamicSlot) -> Expr {
        match slot {
            DynamicSlot::Conditional(conditional) => {
                let mut alternate = conditional
                    .else_slot
                    .as_ref()
                    .map(|slot| self.generate_dynamic_slot_object(element_node, slot))
                    .unwrap_or_else(undefined_expr);

                for branch in conditional.else_if_slots.iter().rev() {
                    alternate = Expr::Cond(CondExpr {
                        span: DUMMY_SP,
                        test: branch.condition.clone(),
                        cons: Box::new(
                            self.generate_dynamic_slot_object(element_node, &branch.slot),
                        ),
                        alt: Box::new(alternate),
                    });
                }

                Expr::Cond(CondExpr {
                    span: DUMMY_SP,
                    test: conditional.if_slot.condition.clone(),
                    cons: Box::new(
                        self.generate_dynamic_slot_object(element_node, &conditional.if_slot.slot),
                    ),
                    alt: Box::new(alternate),
                })
            }

            DynamicSlot::RenderList(render_list) => {
                let Some(Node::Element(carrier)) =
                    element_node.children.get(render_list.slot_template_index)
                else {
                    debug_assert!(
                        false,
                        "Dynamic v-for slot source must be a template element"
                    );
                    return undefined_expr();
                };

                let Some(v_for) = carrier
                    .starting_tag
                    .directives
                    .as_deref()
                    .and_then(|directives| directives.v_for.as_ref())
                else {
                    debug_assert!(false, "Dynamic v-for slot carrier must retain v-for");
                    return undefined_expr();
                };

                let render_item = Expr::Arrow(ArrowExpr {
                    span: v_for.span,
                    ctxt: Default::default(),
                    params: crate::directives::v_for::create_for_loop_params(
                        &v_for.parse_result,
                        1,
                    ),
                    body: Box::new(ArrowFunctionBody::Expr(Box::new(Expr::Paren(ParenExpr {
                        span: DUMMY_SP,
                        expr: Box::new(
                            self.generate_dynamic_slot_object(element_node, &render_list.slot),
                        ),
                    })))),
                    is_async: false,
                    is_generator: false,
                    type_params: None,
                    return_type: None,
                });

                Expr::Call(CallExpr {
                    span: v_for.span,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(
                        self.get_and_add_import_ident(VueImports::RenderList)
                            .into_ident_spanned(v_for.span),
                    ))),
                    args: vec![
                        expr_arg(*v_for.parse_result.source.clone()),
                        expr_arg(render_item),
                    ],
                    type_args: None,
                })
            }
        }
    }

    fn generate_dynamic_slot_object(
        &mut self,
        element_node: &ElementNode,
        slot: &DynamicSlotBuild,
    ) -> Expr {
        let mut props = vec![
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: "name".into(),
                }),
                value: Box::new(str_or_expr_to_expr(&slot.name)),
            }))),
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: "fn".into(),
                }),
                value: self.generate_slot_function(
                    element_node,
                    slot.props.as_deref(),
                    &slot.source,
                ),
            }))),
        ];

        if let Some(key) = slot.key {
            props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: "key".into(),
                }),
                value: Box::new(Expr::Lit(Lit::Num(Number {
                    span: DUMMY_SP,
                    value: key as f64,
                    raw: None,
                }))),
            }))));
        }

        Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props,
        })
    }
}

fn get_slot_source_nodes<'a>(element_node: &'a ElementNode, source: &SlotSource) -> Vec<&'a Node> {
    match source {
        SlotSource::ImplicitDefaultSlot(indices) => indices
            .iter()
            .filter_map(|idx| element_node.children.get(*idx))
            .collect(),

        SlotSource::TemplateSlotChildren(index) => element_node
            .children
            .get(*index)
            .and_then(|node| match node {
                fervid_core::Node::Element(element) => Some(element.children.iter().collect()),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

/// Produces an `undefined` identifier expression
fn undefined_expr() -> Expr {
    Expr::Ident(fervid_atom!("undefined").into_ident())
}

fn str_or_expr_to_expr(value: &StrOrExpr) -> Expr {
    match value {
        StrOrExpr::Str(value) => Expr::Lit(Lit::Str(value.clone().into())),
        StrOrExpr::Expr(value) => *value.clone(),
    }
}
