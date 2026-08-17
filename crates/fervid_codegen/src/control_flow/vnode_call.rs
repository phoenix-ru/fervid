use fervid_core::{
    ArrayExpression, CacheExpression, CallExpression as FervidCallExpression, ElementCodegenNode,
    ElementCodegenValue, ElementNode, ExpressionNode, ExpressionPropNameNode, IntoIdent,
    JsChildNode, ObjectExpression, PropsExpression, SlotBuild, SlotFlag, SlotSource, StrOrExpr,
    VNodeCall, VNodeCallTag, VNodeChildren, VueImports, is_valid_propname,
};
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, ComputedPropName, Expr,
        ExprOrSpread, IdentName, KeyValueProp, Lit, Null, Number, ObjectLit, Prop, PropName,
        PropOrSpread, Str,
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

                Expr::Object(ObjectLit {
                    span: DUMMY_SP,
                    props,
                })
            }
        }
    }

    fn generate_slot_property(
        &mut self,
        element_node: &ElementNode,
        slot: &SlotBuild,
    ) -> PropOrSpread {
        let slot_children = match &slot.source {
            SlotSource::ImplicitDefaultSlot(indices) => indices
                .iter()
                .filter_map(|idx| element_node.children.get(*idx))
                .collect::<Vec<_>>(),
            SlotSource::TemplateSlotChildren(index) => element_node
                .children
                .get(*index)
                .and_then(|node| match node {
                    fervid_core::Node::Element(element) => Some(element.children.iter().collect()),
                    _ => None,
                })
                .unwrap_or_default(),
        };

        let mut generated_children = Vec::new();
        self.generate_node_sequence(
            &mut slot_children.into_iter(),
            &mut generated_children,
            0,
            false,
        );

        let body = Expr::Array(ArrayLit {
            span: DUMMY_SP,
            elems: generated_children
                .into_iter()
                .map(|child| Some(expr_arg(child)))
                .collect(),
        });

        let params = slot
            .props
            .as_ref()
            .map(|props| vec![*props.to_owned()])
            .unwrap_or_default();

        let arrow = Expr::Arrow(ArrowExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            params,
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(body))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        });

        let with_ctx = Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::WithCtx)
                    .into_ident_spanned(DUMMY_SP),
            ))),
            args: vec![expr_arg(arrow)],
            type_args: None,
        });

        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_or_expr_to_prop_name(&slot.name),
            value: Box::new(with_ctx),
        })))
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

fn null_expr() -> Expr {
    Expr::Lit(Lit::Null(Null { span: DUMMY_SP }))
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
            cache: Default::default(),
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
}
