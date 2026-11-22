use fervid_core::{
    ArrayExpression, CacheExpression, CallExpression as FervidCallExpression, ElementCodegenNode,
    ElementCodegenValue, ElementNode, ExpressionNode, ExpressionPropNameNode, IntoIdent,
    JsChildNode, ObjectExpression, PropsExpression, VNodeCall, VNodeCallTag, VueImports,
    WithMemoCall, is_valid_propname,
};
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{
        ArrayLit, CallExpr, Callee, Expr, ExprOrSpread, KeyValueProp, Lit, Number, ObjectLit, Prop,
        PropName, PropOrSpread, Str,
    },
};

use crate::{
    context::CodegenContext,
    utils::{expr_arg, null_expr},
};

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
            ElementCodegenValue::SlotOutletCall(slot_outlet) => {
                self.generate_slot_outlet_call(element, slot_outlet)
            }
            ElementCodegenValue::WithMemoCall(with_memo_call) => {
                self.generate_with_memo_call(element, with_memo_call)
            }
        };

        if codegen_node.cache.is_empty() {
            value
        } else {
            let index = self.allocate_next_cache_entry();
            self.wrap_cache_expression(index, value, codegen_node.cache)
        }
    }

    // Generates `createElementVNode`/`createElementBlock` and its non-element counterparts.
    // Args:
    // 1st - name or Fragment;
    // 2nd (optional) - attributes & directives object;
    // 3rd (optional) - children or slots;
    // 4th (optional) - patch flag;
    // 5th (optional) - props array (for PROPS patch flag).
    fn generate_vnode_call(
        &mut self,
        element_node: &ElementNode,
        vnode_call: &VNodeCall,
        wrap_in_block: bool,
    ) -> Expr {
        let span = DUMMY_SP;
        let tag = self.generate_vnode_tag(&vnode_call.tag, span);
        let props = vnode_call
            .props
            .as_ref()
            .map(|props| self.generate_props_expression(props));
        let children = vnode_call
            .children
            .as_ref()
            .map(|children| self.generate_vnode_children(element_node, children, &vnode_call.tag));
        let patch_flags = vnode_call.patch_hints.flags.bits();
        let dynamic_prop_names = &vnode_call.patch_hints.props;

        let needs_dynamic_props = !dynamic_prop_names.is_empty();
        let needs_patch_flags = needs_dynamic_props || patch_flags != 0;
        let needs_children = needs_patch_flags || children.is_some();
        let needs_props = needs_children || props.is_some();

        let args_len = if needs_dynamic_props {
            5
        } else if needs_patch_flags {
            4
        } else if needs_children {
            3
        } else if needs_props {
            2
        } else {
            1
        };

        // Arguments for function call
        let mut args = Vec::with_capacity(args_len);

        // Arg 1: name (VNodeCallTag). Often a stringified name or Fragment
        args.push(expr_arg(tag));

        // Arg 2 (optional): props expression (default to null)
        if needs_props {
            args.push(expr_arg(props.unwrap_or_else(null_expr)));
        }

        // Arg 3 (optional): component children expression (default to null).
        // This may be a text concatenation, an array of child nodes, or `null`.
        if needs_children {
            args.push(expr_arg(children.unwrap_or_else(null_expr)));
        }

        // Arg 4 (optional): patch flags (default to 0)
        if needs_patch_flags {
            args.push(expr_arg(Expr::Lit(Lit::Num(Number {
                span,
                value: patch_flags.into(),
                raw: None,
            }))));
        }

        // Arg 5 (optional): dynamic prop names (default to nothing)
        if needs_dynamic_props {
            args.push(expr_arg(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems: dynamic_prop_names
                    .iter()
                    .map(|dynamic_prop_name| {
                        Some(ExprOrSpread {
                            expr: Box::new(Expr::Lit(Lit::Str(Str {
                                span: DUMMY_SP,
                                value: dynamic_prop_name.to_owned().into(),
                                raw: None,
                            }))),
                            spread: None,
                        })
                    })
                    .collect(),
            })))
        }

        // When wrapping in block, `createElementBlock` is used, otherwise `createElementVNode`.
        // Same for components, but just `createBlock` and `createVNode`
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

        // Examples:
        // `createElementVNode("element-name", {element:attrs}, [element, children], PATCH_FLAGS)`
        // `createVNode(_component_name, {component:attrs}, {component:slots}, PATCH_FLAGS)`
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

        // When wrapping in block, we also need `openBlock()`
        let mut expr = if vnode_call.is_block || wrap_in_block {
            // E.g. (openBlock(), createElementBlock("element-name", {element:attrs}, [element, children], PATCH_FLAGS))
            self.wrap_in_open_block(call, span)
        } else {
            // Just VNode call
            call
        };

        // Process directives
        if let Some(directives) = vnode_call.directives.as_ref() {
            expr = self.maybe_generate_with_directives(expr, directives.elems.clone());
        }

        expr
    }

    fn generate_with_memo_call(
        &mut self,
        element_node: &ElementNode,
        with_memo_call: &WithMemoCall,
    ) -> Expr {
        let vnode_call = self.generate_vnode_call(element_node, &with_memo_call.vnode_call, false);
        self.generate_v_memo(with_memo_call.expr.to_owned(), Box::new(vnode_call))
    }

    fn generate_vnode_tag(&mut self, tag: &VNodeCallTag, span: Span) -> Expr {
        match tag {
            VNodeCallTag::Expr(expr) => *expr.to_owned(),
            VNodeCallTag::CallExpression(call) => self.generate_call_expression(call),
            VNodeCallTag::Builtin(builtin) => Expr::Ident(
                self.get_and_add_import_ident(VueImports::from(*builtin))
                    .into_ident_spanned(span),
            ),
            VNodeCallTag::Fragment => Expr::Ident(
                self.get_and_add_import_ident(VueImports::Fragment)
                    .into_ident_spanned(span),
            ),
        }
    }

    pub(crate) fn generate_props_expression(&mut self, props: &PropsExpression) -> Expr {
        match props {
            PropsExpression::ObjectExpression(object) => self.generate_object_expression(object),
            PropsExpression::CallExpression(call) => self.generate_call_expression(call),
            PropsExpression::ExpressionNode(expr) => self.generate_expression_node(expr),
        }
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

    pub(crate) fn generate_cache_expression(&mut self, cache: &CacheExpression) -> Expr {
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

fn expression_prop_name_to_prop_name(value: &ExpressionPropNameNode) -> PropName {
    match value {
        ExpressionPropNameNode::SimpleExpression(expr) => {
            if is_valid_propname(&expr.ast.sym) {
                PropName::Ident(expr.ast.to_owned())
            } else {
                PropName::Str(Str {
                    span: expr.ast.span,
                    value: expr.ast.sym.to_owned().into(),
                    raw: None,
                })
            }
        }
        ExpressionPropNameNode::CompoundExpression(expr) => expr.ast.to_owned(),
    }
}

// Tests live under dedicated tests/ sub-directory
