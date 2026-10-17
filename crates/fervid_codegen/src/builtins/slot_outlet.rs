use fervid_core::{
    ElementNode, IntoIdent, SlotOutletCall, SlotOutletFallback, VueImports, fervid_atom,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread, MemberExpr,
        MemberProp, ObjectLit,
    },
};

use crate::CodegenContext;

impl CodegenContext {
    pub fn generate_slot_outlet_call(
        &mut self,
        element_node: &ElementNode,
        slot_outlet_call: &SlotOutletCall,
    ) -> Expr {
        let capacity = if slot_outlet_call.branch_key.is_some() {
            6
        } else if slot_outlet_call.no_slotted {
            5
        } else if !element_node.children.is_empty() {
            4
        } else if slot_outlet_call.props.is_some() {
            3
        } else {
            2
        };

        let mut slot_args = Vec::with_capacity(capacity);

        // TODO prefixIdentifiers
        // 1: _ctx.$slots
        slot_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(Expr::Ident(fervid_atom!("_ctx").into_ident())),
                prop: MemberProp::Ident(fervid_atom!("$slots").into()),
            })),
        });

        // 2: Slot name
        slot_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(slot_outlet_call.name.to_owned().into()),
        });

        // 3: Slot props
        if let Some(ref slot_props) = slot_outlet_call.props {
            let props_expr = self.generate_props_expression(slot_props);
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(props_expr),
            });
        } else if capacity > 3 {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(ObjectLit {
                    span: slot_outlet_call.span,
                    props: vec![],
                })),
            });
        }

        // 4: Fallback
        if let SlotOutletFallback::UseChildren = slot_outlet_call.fallback {
            let (children, _) = self.generate_element_children(element_node, true);
            let children = Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: children
                    .into_iter()
                    .map(|child| {
                        Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(child),
                        })
                    })
                    .collect(),
            });

            // Wrap as function
            // () => children
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Arrow(ArrowExpr {
                    span: slot_outlet_call.span,
                    ctxt: Default::default(),
                    params: vec![],
                    body: Box::new(BlockStmtOrExpr::Expr(Box::new(children))),
                    is_async: false,
                    is_generator: false,
                    type_params: None,
                    return_type: None,
                })),
            });
        } else if capacity > 4 {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(fervid_atom!("undefined").into_ident())),
            });
        }

        // 5: noSlotted
        if slot_outlet_call.no_slotted {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(true.into())),
            });
        } else if capacity > 5 {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(fervid_atom!("undefined").into_ident())),
            });
        }

        // 6: Slot branch key
        if let Some(ref branch_key) = slot_outlet_call.branch_key {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: branch_key.to_owned(),
            });
        }

        Expr::Call(CallExpr {
            span: slot_outlet_call.span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::RenderSlot)
                    .into_ident(),
            ))),
            args: slot_args,
            type_args: None,
        })
    }
}

#[cfg(all(test, feature = "new-pipeline"))]
mod tests {
    use fervid_core::{
        AttributeOrBinding, BuiltinType, ElementCodegenValue, ElementKind, ElementNode, Node,
        SfcDescriptor, SfcStyleBlock, StartingTag, StrOrExpr, VBindDirective, VCustomDirective,
        VueDirectives,
    };
    use fervid_transform::{
        TransformSfcContext, TransformSfcOptions,
        error::{TemplateErrorKind, TransformError},
        template::{
            core::{
                transform_expression::pre_transform_expression,
                transform_slot_outlet::pre_transform_slot_outlet,
            },
            node_transforms::TransformNodeState,
        },
    };
    use swc_core::common::{BytePos, DUMMY_SP, Span};

    use crate::{
        CodegenContext,
        test_utils::{js, to_str},
    };

    // Vue source: https://github.com/vuejs/core/blob/4ab865a848a1da3d10fb674f857e5fff13094644/packages/compiler-core/__tests__/transforms/transformSlotOutlet.spec.ts

    #[test]
    fn default_slot_outlet() {
        assert_eq!(
            // <slot />
            transform_and_generate_slot_outlet(vec![]),
            "_renderSlot(_ctx.$slots,\"default\")"
        );
    }

    #[test]
    fn statically_named_slot_outlet() {
        assert_eq!(
            // <slot name="foo" />
            transform_and_generate_slot_outlet(vec![regular_attribute("name", Some("foo"))]),
            "_renderSlot(_ctx.$slots,\"foo\")"
        );
    }

    #[test]
    fn dynamically_named_slot_outlet() {
        assert_eq!(
            // <slot :name="foo" />
            transform_and_generate_slot_outlet(vec![v_bind_attribute("name", "foo")]),
            "_renderSlot(_ctx.$slots,_ctx.foo)"
        );
    }

    #[test]
    fn dynamically_named_slot_outlet_with_prefix_identifiers_true() {
        assert_eq!(
            // <slot :name="foo + bar" />
            transform_and_generate_slot_outlet(vec![v_bind_attribute("name", "foo + bar")]),
            "_renderSlot(_ctx.$slots,_ctx.foo+_ctx.bar)"
        );
    }

    #[test]
    fn default_slot_outlet_with_props() {
        assert_eq!(
            // <slot foo="bar" :baz="qux" :foo-bar="foo-bar" />
            transform_and_generate_slot_outlet(vec![
                regular_attribute("foo", Some("bar")),
                v_bind_attribute("baz", "qux"),
                v_bind_attribute("foo-bar", "foo-bar"),
            ]),
            "_renderSlot(_ctx.$slots,\"default\",{foo:\"bar\",baz:_ctx.qux,fooBar:_ctx.foo-_ctx.bar})"
        );
    }

    #[test]
    fn statically_named_slot_outlet_with_props() {
        assert_eq!(
            // <slot name="foo" foo="bar" :baz="qux" />
            transform_and_generate_slot_outlet(vec![
                regular_attribute("name", Some("foo")),
                regular_attribute("foo", Some("bar")),
                v_bind_attribute("baz", "qux"),
            ]),
            "_renderSlot(_ctx.$slots,\"foo\",{foo:\"bar\",baz:_ctx.qux})"
        );
    }

    #[test]
    fn dynamically_named_slot_outlet_with_props() {
        assert_eq!(
            // <slot :name="foo" foo="bar" :baz="qux" />
            transform_and_generate_slot_outlet(vec![
                v_bind_attribute("name", "foo"),
                regular_attribute("foo", Some("bar")),
                v_bind_attribute("baz", "qux"),
            ]),
            "_renderSlot(_ctx.$slots,_ctx.foo,{foo:\"bar\",baz:_ctx.qux})"
        );
    }

    #[test]
    fn it_ignores_valueless_regular_slot_props() {
        assert_eq!(
            // <slot foo />
            transform_and_generate_slot_outlet(vec![regular_attribute("foo", None)]),
            "_renderSlot(_ctx.$slots,\"default\")"
        );
    }

    #[test]
    fn it_preserves_explicitly_empty_regular_slot_props() {
        assert_eq!(
            // <slot foo="" />
            transform_and_generate_slot_outlet(vec![regular_attribute("foo", Some(""))]),
            "_renderSlot(_ctx.$slots,\"default\",{foo:\"\"})"
        );
    }

    #[test]
    fn it_ignores_valueless_slot_name() {
        assert_eq!(
            // <slot name />
            transform_and_generate_slot_outlet(vec![regular_attribute("name", None)]),
            "_renderSlot(_ctx.$slots,\"default\")"
        );
    }

    #[test]
    fn it_preserves_explicitly_empty_slot_name() {
        assert_eq!(
            // <slot name="" />
            transform_and_generate_slot_outlet(vec![regular_attribute("name", Some(""))]),
            "_renderSlot(_ctx.$slots,\"\")"
        );
    }

    #[test]
    fn default_slot_outlet_with_fallback() {
        assert_eq!(
            // <slot><div /></slot>
            transform_and_generate_slot_outlet_with(vec![], vec![element("div")], false, false),
            "_renderSlot(_ctx.$slots,\"default\",{},()=>[_createElementVNode(\"div\")])"
        );
    }

    #[test]
    fn named_slot_outlet_with_fallback() {
        assert_eq!(
            // <slot name="foo"><div /></slot>
            transform_and_generate_slot_outlet_with(
                vec![regular_attribute("name", Some("foo"))],
                vec![element("div")],
                false,
                false,
            ),
            "_renderSlot(_ctx.$slots,\"foo\",{},()=>[_createElementVNode(\"div\")])"
        );
    }

    #[test]
    fn default_slot_outlet_with_props_and_fallback() {
        assert_eq!(
            // <slot :foo="bar"><div /></slot>
            transform_and_generate_slot_outlet_with(
                vec![v_bind_attribute("foo", "bar")],
                vec![element("div")],
                false,
                false,
            ),
            "_renderSlot(_ctx.$slots,\"default\",{foo:_ctx.bar},()=>[_createElementVNode(\"div\")])"
        );
    }

    #[test]
    fn named_slot_outlet_with_props_and_fallback() {
        assert_eq!(
            // <slot name="foo" :foo="bar"><div /></slot>
            transform_and_generate_slot_outlet_with(
                vec![
                    regular_attribute("name", Some("foo")),
                    v_bind_attribute("foo", "bar"),
                ],
                vec![element("div")],
                false,
                false,
            ),
            "_renderSlot(_ctx.$slots,\"foo\",{foo:_ctx.bar},()=>[_createElementVNode(\"div\")])"
        );
    }

    #[test]
    fn slot_with_slotted_false() {
        assert_eq!(
            // <slot />
            // slotted: false
            transform_and_generate_slot_outlet_with(vec![], vec![], true, false),
            "_renderSlot(_ctx.$slots,\"default\",{},undefined,true)"
        );
        assert_eq!(
            // <slot>fallback</slot>
            // slotted: false
            transform_and_generate_slot_outlet_with(
                vec![],
                vec![Node::Text("fallback".into(), DUMMY_SP)],
                true,
                false,
            ),
            "_renderSlot(_ctx.$slots,\"default\",{},()=>[\"fallback\"],true)"
        );
    }

    #[test]
    fn error_on_unexpected_custom_directive_on_slot() {
        let directive_span = Span::new(BytePos(7), BytePos(12));
        let directives = VueDirectives {
            custom: vec![VCustomDirective {
                name: "foo".into(),
                span: directive_span,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut node = slot_outlet(vec![], vec![]);
        let Node::Element(element) = &mut node else {
            panic!("slot outlet must be an element");
        };
        element.starting_tag.directives = Some(Box::new(directives));
        let descriptor = SfcDescriptor::default();
        let options = transform_options();
        let mut ctx = TransformSfcContext::new(&descriptor, &options);
        let mut state = TransformNodeState::default();

        // <slot v-foo />
        pre_transform_slot_outlet(&mut ctx, &mut state, &mut node);

        assert!(matches!(
            ctx.errors.as_slice(),
            [TransformError::TemplateError(error)]
                if matches!(error.kind, TemplateErrorKind::VSlotUnexpectedDirectiveOnSlotOutlet)
                    && error.span == directive_span
        ));
    }

    #[test]
    fn dynamically_named_slot_outlet_with_v_bind_shorthand() {
        assert_eq!(
            // <slot :name />
            transform_and_generate_slot_outlet(vec![v_bind_attribute("name", "name")]),
            "_renderSlot(_ctx.$slots,_ctx.name)"
        );
    }

    #[test]
    fn it_keeps_slotted_scope_for_scoped_styles_with_slotted_usage() {
        assert_eq!(
            // <slot />
            transform_and_generate_slot_outlet_with(vec![], vec![], true, true),
            "_renderSlot(_ctx.$slots,\"default\")"
        );
    }

    #[test]
    fn it_preserves_props_position_when_disabling_slotted_scope() {
        assert_eq!(
            // <slot foo="bar" />
            // slotted: false
            transform_and_generate_slot_outlet_with(
                vec![regular_attribute("foo", Some("bar"))],
                vec![],
                true,
                false,
            ),
            "_renderSlot(_ctx.$slots,\"default\",{foo:\"bar\"},undefined,true)"
        );
    }

    #[test]
    fn it_generates_text_fallback_as_an_array() {
        assert_eq!(
            // <slot>fallback</slot>
            transform_and_generate_slot_outlet_with(
                vec![],
                vec![Node::Text("fallback".into(), DUMMY_SP)],
                false,
                false,
            ),
            "_renderSlot(_ctx.$slots,\"default\",{},()=>[\"fallback\"])"
        );
    }

    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vFor.spec.ts#L921-L969
    #[test]
    fn template_v_for_key_injection_with_single_slot_child() {
        assert_eq!(
            // <template v-for="item in items" :key="item.id"><slot /></template>
            // slotted: false
            transform_and_generate_slot_outlet_with_branch_key(
                vec![],
                vec![],
                false,
                false,
                "item.id"
            ),
            "_renderSlot(_ctx.$slots,\"default\",{},undefined,undefined,item.id)"
        );
    }

    #[test]
    fn template_v_for_key_injection_with_slot_fallback() {
        assert_eq!(
            // <template v-for="item in items" :key="item.id"><slot>fallback</slot></template>
            // slotted: false
            transform_and_generate_slot_outlet_with_branch_key(
                vec![],
                vec![Node::Text("fallback".into(), DUMMY_SP)],
                false,
                false,
                "item.id",
            ),
            "_renderSlot(_ctx.$slots,\"default\",{},()=>[\"fallback\"],undefined,item.id)"
        );
    }

    #[test]
    fn template_v_for_key_injection_with_slot_props() {
        assert_eq!(
            // <template v-for="item in items" :key="item.id"><slot foo="bar" /></template>
            // slotted: false
            transform_and_generate_slot_outlet_with_branch_key(
                vec![regular_attribute("foo", Some("bar"))],
                vec![],
                false,
                false,
                "item.id",
            ),
            "_renderSlot(_ctx.$slots,\"default\",{foo:\"bar\"},undefined,undefined,item.id)"
        );
    }

    #[test]
    fn template_v_for_key_injection_with_no_slotted_scope() {
        assert_eq!(
            // <template v-for="item in items" :key="item.id"><slot /></template>
            // slotted: false
            transform_and_generate_slot_outlet_with_branch_key(
                vec![],
                vec![],
                true,
                false,
                "item.id"
            ),
            "_renderSlot(_ctx.$slots,\"default\",{},undefined,true,item.id)"
        );
    }

    fn transform_and_generate_slot_outlet(attributes: Vec<AttributeOrBinding>) -> String {
        transform_and_generate_slot_outlet_with(attributes, vec![], false, false)
    }

    fn transform_and_generate_slot_outlet_with(
        attributes: Vec<AttributeOrBinding>,
        children: Vec<Node>,
        has_scoped_styles: bool,
        slotted: bool,
    ) -> String {
        transform_and_generate_slot_outlet_with_optional_branch_key(
            attributes,
            children,
            has_scoped_styles,
            slotted,
            None,
        )
    }

    fn transform_and_generate_slot_outlet_with_branch_key(
        attributes: Vec<AttributeOrBinding>,
        children: Vec<Node>,
        has_scoped_styles: bool,
        slotted: bool,
        branch_key: &str,
    ) -> String {
        transform_and_generate_slot_outlet_with_optional_branch_key(
            attributes,
            children,
            has_scoped_styles,
            slotted,
            Some(branch_key),
        )
    }

    fn transform_and_generate_slot_outlet_with_optional_branch_key(
        attributes: Vec<AttributeOrBinding>,
        children: Vec<Node>,
        has_scoped_styles: bool,
        slotted: bool,
        branch_key: Option<&str>,
    ) -> String {
        let mut node = slot_outlet(attributes, children);
        let descriptor = SfcDescriptor {
            styles: has_scoped_styles
                .then(|| SfcStyleBlock {
                    lang: "css".into(),
                    content: "".into(),
                    is_scoped: true,
                    is_module: false,
                    span: DUMMY_SP,
                })
                .into_iter()
                .collect(),
            slotted,
            ..Default::default()
        };
        let options = transform_options();
        let mut ctx = TransformSfcContext::new(&descriptor, &options);
        let mut state = TransformNodeState::default();
        pre_transform_expression(&mut ctx, &state, &mut node);
        pre_transform_slot_outlet(&mut ctx, &mut state, &mut node);

        if let Some(branch_key) = branch_key {
            let Node::Element(element) = &mut node else {
                panic!("slot outlet must be an element");
            };
            let Some(codegen_node) = element.codegen_node.as_mut() else {
                panic!("slot outlet must have a codegen node");
            };
            let ElementCodegenValue::SlotOutletCall(slot_call) = &mut codegen_node.value else {
                panic!("slot outlet must generate renderSlot");
            };
            slot_call.branch_key = Some(js(branch_key));
        }

        let mut codegen = CodegenContext::default();
        to_str(codegen.generate_node(&node, false))
    }

    fn transform_options() -> TransformSfcOptions<'static> {
        TransformSfcOptions {
            is_prod: false,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        }
    }

    fn slot_outlet(attributes: Vec<AttributeOrBinding>, children: Vec<Node>) -> Node {
        Node::Element(ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "slot".into(),
                attributes,
                directives: None,
            },
            children,
            ElementKind::Builtin(BuiltinType::Slot),
        ))
    }

    fn element(tag_name: &str) -> Node {
        Node::Element(ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: tag_name.into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Element,
        ))
    }

    fn regular_attribute(name: &str, value: Option<&str>) -> AttributeOrBinding {
        AttributeOrBinding::RegularAttribute {
            name: name.into(),
            value: value.map(Into::into),
            span: DUMMY_SP,
        }
    }

    fn v_bind_attribute(name: &str, value: &str) -> AttributeOrBinding {
        AttributeOrBinding::VBind(VBindDirective {
            argument: Some(StrOrExpr::Str(name.into())),
            value: js(value),
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: DUMMY_SP,
        })
    }
}
