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
        // TODO Support ctx.scope_id and ctx.slotted
        let capacity = if !element_node.children.is_empty() {
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

        //4: Fallback
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
        }

        // TODO Add `true` when context.scope_id && !context.slotted

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
        AttributeOrBinding, BuiltinType, ElementKind, ElementNode, Node, SfcDescriptor, StartingTag,
    };
    use fervid_transform::{
        TransformSfcContext, TransformSfcOptions,
        template::{
            core::transform_slot_outlet::pre_transform_slot_outlet,
            node_transforms::TransformNodeState,
        },
    };
    use swc_core::common::DUMMY_SP;

    use crate::{CodegenContext, test_utils::to_str};

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

    fn transform_and_generate_slot_outlet(attributes: Vec<AttributeOrBinding>) -> String {
        let mut node = Node::Element(ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "slot".into(),
                attributes,
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Slot),
        ));
        let descriptor = SfcDescriptor::default();
        let options = TransformSfcOptions {
            is_prod: false,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        };
        let mut ctx = TransformSfcContext::new(&descriptor, &options);
        let mut state = TransformNodeState::default();

        pre_transform_slot_outlet(&mut ctx, &mut state, &mut node);

        let mut codegen = CodegenContext::default();
        to_str(codegen.generate_node(&node, false))
    }

    fn regular_attribute(name: &str, value: Option<&str>) -> AttributeOrBinding {
        AttributeOrBinding::RegularAttribute {
            name: name.into(),
            value: value.map(Into::into),
            span: DUMMY_SP,
        }
    }
}
