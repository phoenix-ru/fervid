use fervid_core::{
    ArrayExpression, CacheExpression, CallExpression as FervidCallExpression, DynamicSlot,
    DynamicSlotBuild, ElementCodegenNode, ElementCodegenValue, ElementNode, ExpressionNode,
    ExpressionPropNameNode, IntoIdent, JsChildNode, Node, ObjectExpression, PropsExpression,
    SlotBuild, SlotFlag, SlotSource, StrOrExpr, VNodeCall, VNodeCallTag, VNodeChildren, VueImports,
    fervid_atom, is_valid_propname,
};
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, ComputedPropName, CondExpr, Expr,
        ExprOrSpread, IdentName, KeyValueProp, Lit, Null, Number, ObjectLit, ParenExpr, Pat, Prop,
        PropName, PropOrSpread, Str,
    },
};

use crate::context::CodegenContext;

impl CodegenContext {
    pub(crate) fn generate_element_codegen_node(
        &mut self,
        element: &ElementNode,
        codegen_node: &ElementCodegenNode,
        wrap_in_block: bool,
    ) -> Expr {
        let value = match &codegen_node.value {
            ElementCodegenValue::VNodeCall(vnode) => {
                self.generate_vnode_call(element, vnode, wrap_in_block)
            }
        };

        if codegen_node.cache.is_empty() {
            value
        } else {
            let index = self.allocate_next_cache_entry();
            self.wrap_cache_expression(index, value, codegen_node.cache)
        }
    }

    fn generate_vnode_call(
        &mut self,
        element_node: &ElementNode,
        vnode_call: &VNodeCall,
        wrap_in_block: bool,
    ) -> Expr {
        let span = DUMMY_SP;
        // TODO(new-pipeline): structural transforms (`transformIf`/`transformFor`) should create
        // fragment VNodeCalls directly. This keeps old Fervid template-carrier behavior until then.
        let should_generate_fragment_instead = (wrap_in_block
            && element_node.starting_tag.tag_name == "template")
            || self.should_generate_fragment(element_node);
        let tag = if should_generate_fragment_instead {
            Expr::Ident(
                self.get_and_add_import_ident(VueImports::Fragment)
                    .into_ident_spanned(span),
            )
        } else {
            self.generate_vnode_tag(&vnode_call.tag, span)
        };
        let props = vnode_call
            .props
            .as_ref()
            .map(|props| self.generate_props_expression(props));
        let children = vnode_call
            .children
            .as_ref()
            .map(|children| self.generate_vnode_children(element_node, children));
        let patch_flags = vnode_call.patch_hints.flags.bits();
        let dynamic_prop_names = &vnode_call.patch_hints.props;

        let mut args = vec![expr_arg(tag)];
        let needs_props = props.is_some() || children.is_some() || patch_flags != 0;
        let needs_children = children.is_some() || patch_flags != 0;

        if needs_props {
            args.push(expr_arg(props.unwrap_or_else(null_expr)));
        }

        if needs_children {
            args.push(expr_arg(children.unwrap_or_else(null_expr)));
        }

        if patch_flags != 0 {
            args.push(expr_arg(Expr::Lit(Lit::Num(Number {
                span,
                value: patch_flags.into(),
                raw: None,
            }))));
        }

        if !dynamic_prop_names.is_empty() {
            args.push(expr_arg(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: dynamic_prop_names
                    .iter()
                    .map(|dynamic_prop_name| {
                        Some(ExprOrSpread {
                            expr: Box::new(Expr::Lit(Lit::Str(Str {
                                span: DUMMY_SP,
                                value: dynamic_prop_name.to_owned(),
                                raw: None,
                            }))),
                            spread: None,
                        })
                    })
                    .collect(),
            })))
        }

        let callee = if vnode_call.is_component {
            if vnode_call.is_block || wrap_in_block {
                VueImports::CreateBlock
            } else {
                VueImports::CreateVNode
            }
        } else if vnode_call.is_block || wrap_in_block {
            VueImports::CreateElementBlock
        } else {
            VueImports::CreateElementVNode
        };

        let call = Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(callee)
                    .into_ident_spanned(span),
            ))),
            args,
            type_args: None,
        });

        let mut expr = if vnode_call.is_block || wrap_in_block {
            self.wrap_in_open_block(call, span)
        } else {
            call
        };

        if let Some(directives) = vnode_call.directives.as_ref() {
            expr = self.maybe_generate_with_directives(expr, directives.elems.clone());
        }

        expr
    }

    fn generate_vnode_tag(&mut self, tag: &VNodeCallTag, span: Span) -> Expr {
        match tag {
            VNodeCallTag::Expr(expr) => *expr.to_owned(),
            VNodeCallTag::CallExpression(call) => self.generate_call_expression(call),
            VNodeCallTag::Builtin(builtin) => Expr::Ident(
                self.get_and_add_import_ident(VueImports::from(*builtin))
                    .into_ident_spanned(span),
            ),
        }
    }

    fn generate_props_expression(&mut self, props: &PropsExpression) -> Expr {
        match props {
            PropsExpression::ObjectExpression(object) => self.generate_object_expression(object),
            PropsExpression::CallExpression(call) => self.generate_call_expression(call),
            PropsExpression::ExpressionNode(expr) => self.generate_expression_node(expr),
        }
    }

    fn generate_vnode_children(
        &mut self,
        element_node: &ElementNode,
        children: &VNodeChildren,
    ) -> Expr {
        match children {
            VNodeChildren::UseElementChildren => {
                let (mut children, was_inlined) =
                    self.generate_element_children(element_node, true);
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
            key: str_or_expr_to_prop_name(&slot.name),
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
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(body))),
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
                    body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Paren(ParenExpr {
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

    fn generate_js_child_node(&mut self, node: &JsChildNode) -> Expr {
        match node {
            JsChildNode::CallExpression(call) => self.generate_call_expression(call),
            JsChildNode::ObjectExpression(object) => self.generate_object_expression(object),
            JsChildNode::ExpressionNode(expr) => self.generate_expression_node(expr),
            JsChildNode::ArrayExpression(array) => self.generate_array_expression(array),
            JsChildNode::CacheExpression(cache) => self.generate_cache_expression(cache),
        }
    }

    fn generate_call_expression(&mut self, call: &FervidCallExpression) -> Expr {
        Expr::Call(CallExpr {
            span: call.span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(call.callee)
                    .into_ident_spanned(call.span),
            ))),
            args: call
                .arguments
                .iter()
                .map(|arg| expr_arg(self.generate_js_child_node(arg)))
                .collect(),
            type_args: None,
        })
    }

    fn generate_object_expression(&mut self, object: &ObjectExpression) -> Expr {
        Expr::Object(ObjectLit {
            span: object.span,
            props: object
                .properties
                .iter()
                .map(|prop| {
                    PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                        key: expression_prop_name_to_prop_name(&prop.key),
                        value: Box::new(self.generate_js_child_node(&prop.value)),
                    })))
                })
                .collect(),
        })
    }

    fn generate_array_expression(&mut self, array: &ArrayExpression) -> Expr {
        Expr::Array(ArrayLit {
            span: array.span,
            elems: array
                .elements
                .iter()
                .map(|elem| Some(expr_arg(self.generate_js_child_node(elem))))
                .collect(),
        })
    }

    fn generate_cache_expression(&mut self, cache: &CacheExpression) -> Expr {
        let index = self.allocate_next_cache_entry();
        let value = self.generate_js_child_node(&cache.value);
        self.wrap_cache_expression(index, value, cache.markers)
    }

    fn generate_expression_node(&mut self, expr: &ExpressionNode) -> Expr {
        match expr {
            ExpressionNode::SimpleExpression(expr) => *expr.ast.to_owned(),
            ExpressionNode::CompoundExpression(expr) => *expr.ast.to_owned(),
        }
    }
}

fn expr_arg(expr: Expr) -> ExprOrSpread {
    ExprOrSpread {
        spread: None,
        expr: Box::new(expr),
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

fn null_expr() -> Expr {
    Expr::Lit(Lit::Null(Null { span: DUMMY_SP }))
}

fn undefined_expr() -> Expr {
    Expr::Ident(fervid_atom!("undefined").into_ident())
}

fn str_or_expr_to_prop_name(value: &StrOrExpr) -> PropName {
    match value {
        StrOrExpr::Str(value) => PropName::Str(value.to_owned()),
        StrOrExpr::Expr(expr) => PropName::Computed(ComputedPropName {
            span: DUMMY_SP,
            expr: expr.to_owned(),
        }),
    }
}

fn str_or_expr_to_expr(value: &StrOrExpr) -> Expr {
    match value {
        StrOrExpr::Str(value) => Expr::Lit(Lit::Str(value.clone())),
        StrOrExpr::Expr(value) => *value.clone(),
    }
}

fn expression_prop_name_to_prop_name(value: &ExpressionPropNameNode) -> PropName {
    match value {
        ExpressionPropNameNode::SimpleExpression(expr) => {
            if is_valid_propname(&expr.ast.sym) {
                PropName::Ident(expr.ast.to_owned())
            } else {
                PropName::Str(Str {
                    span: expr.ast.span,
                    value: expr.ast.sym.to_owned(),
                    raw: None,
                })
            }
        }
        ExpressionPropNameNode::CompoundExpression(expr) => expr.ast.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{CompoundExpressionNode, ExpressionNode};

    #[cfg(feature = "new-pipeline")]
    use fervid_core::{
        ElementKind, ElementNode, ForParseResult, Interpolation, Node, PatchHints, SfcDescriptor,
        SfcTemplateBlock, StartingTag, StrOrExpr, VForDirective, VSlotDirective, VueDirectives,
    };
    #[cfg(feature = "new-pipeline")]
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{js, to_str};

    use super::*;

    #[test]
    fn it_generates_cache_expressions() {
        let cache = CacheExpression {
            value: JsChildNode::ExpressionNode(Box::new(ExpressionNode::CompoundExpression(
                CompoundExpressionNode {
                    ast: js("(...args) => handler(...args)"),
                    is_handler_key: false,
                },
            ))),
            markers: Default::default(),
        };
        let mut ctx = CodegenContext::default();

        assert_eq!(
            to_str(ctx.generate_cache_expression(&cache)),
            "_cache[0]||(_cache[0]=(...args)=>handler(...args))"
        );
        assert_eq!(
            to_str(ctx.generate_cache_expression(&cache)),
            "_cache[1]||(_cache[1]=(...args)=>handler(...args))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_static_slots() {
        assert_eq!(
            // <template v-slot:header>static</template>
            transform_and_generate_slots(vec![slot("header", "static", None)]),
            "(_openBlock(),_createBlock(_component_Comp,null,{\"header\":_withCtx(()=>[_createTextVNode(\"static\")]),_:1}))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_dynamic_slots() {
        assert_eq!(
            // <template v-if="ok" v-slot:header>dynamic</template>
            transform_and_generate_slots(vec![slot("header", "dynamic", Some("ok"))]),
            "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({_:2},[_ctx.ok?{name:\"header\",fn:_withCtx(()=>[_createTextVNode(\"dynamic\")]),key:0}:undefined]),1024))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_static_and_dynamic_slots() {
        assert_eq!(
            transform_and_generate_slots(vec![
                // <template v-slot:header>static</template>
                slot("header", "static", None),
                // <template v-if="ok" v-slot:footer>dynamic</template>
                slot("footer", "dynamic", Some("ok")),
            ]),
            "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({\"header\":_withCtx(()=>[_createTextVNode(\"static\")]),_:2},[_ctx.ok?{name:\"footer\",fn:_withCtx(()=>[_createTextVNode(\"dynamic\")]),key:0}:undefined]),1024))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_conditional_slot_chain() {
        assert_eq!(
            transform_and_generate_slots(vec![
                // <template v-if="ok" v-slot:one>one</template>
                slot_with_condition("one", "one", Some("ok"), None, false),
                // <template v-else-if="other" v-slot:two>two</template>
                slot_with_condition("two", "two", None, Some("other"), false),
                // <template v-else v-slot:three>three</template>
                slot_with_condition("three", "three", None, None, true),
            ]),
            "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({_:2},[_ctx.ok?{name:\"one\",fn:_withCtx(()=>[_createTextVNode(\"one\")]),key:0}:_ctx.other?{name:\"two\",fn:_withCtx(()=>[_createTextVNode(\"two\")]),key:1}:{name:\"three\",fn:_withCtx(()=>[_createTextVNode(\"three\")]),key:2}]),1024))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_generates_v_for_dynamic_slots() {
        assert_eq!(
            // <template v-for="item in items" v-slot:[item.name]>{{ item.value }}</template>
            transform_and_generate_slots(vec![v_for_slot()]),
            "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({_:2},[_renderList(_ctx.items,(item)=>({name:item.name,fn:_withCtx(()=>[_createTextVNode(_toDisplayString(item.value),1)])}))]),1024))"
        );
    }

    #[cfg(feature = "new-pipeline")]
    fn transform_and_generate_slots(children: Vec<Node>) -> String {
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "Comp".into(),
                    attributes: vec![],
                    directives: None,
                },
                children,
                template_scope: 0,
                tag_type: ElementKind::Component,
                patch_hints: PatchHints::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        };
        let descriptor = SfcDescriptor::default();
        let options = fervid_transform::TransformSfcOptions {
            is_prod: false,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        };
        let mut transform_ctx = fervid_transform::TransformSfcContext::new(&descriptor, &options);
        fervid_transform::template::transform_and_record_template(
            &mut template,
            &mut transform_ctx,
        );
        assert!(transform_ctx.errors.is_empty());

        let mut codegen_ctx = CodegenContext::default();
        to_str(codegen_ctx.generate_node(&template.roots[0], true))
    }

    #[cfg(feature = "new-pipeline")]
    fn slot(name: &str, text: &str, condition: Option<&str>) -> Node {
        slot_with_condition(name, text, condition, None, false)
    }

    #[cfg(feature = "new-pipeline")]
    fn slot_with_condition(
        name: &str,
        text: &str,
        v_if: Option<&str>,
        v_else_if: Option<&str>,
        v_else: bool,
    ) -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_else: v_else.then_some(()),
                    v_else_if: v_else_if.map(js),
                    v_if: v_if.map(js),
                    v_slot: Some(VSlotDirective {
                        slot_name: Some(name.into()),
                        value: None,
                    }),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text(text.into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Template,
            patch_hints: PatchHints::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })
    }

    #[cfg(feature = "new-pipeline")]
    fn v_for_slot() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_for: Some(VForDirective {
                        parse_result: Box::new(ForParseResult {
                            source: js("items"),
                            value: js("item"),
                            key: None,
                            index: None,
                            finalized: false,
                            finalized_is_dynamic: false,
                        }),
                        patch_flags: Default::default(),
                        span: DUMMY_SP,
                    }),
                    v_slot: Some(VSlotDirective {
                        slot_name: Some(StrOrExpr::Expr(js("item.name"))),
                        value: None,
                    }),
                    ..Default::default()
                })),
            },
            children: vec![Node::Interpolation(Interpolation {
                value: js("item.value"),
                template_scope: 0,
                patch_flag: false,
                span: DUMMY_SP,
            })],
            template_scope: 0,
            tag_type: ElementKind::Template,
            patch_hints: PatchHints::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })
    }
}
