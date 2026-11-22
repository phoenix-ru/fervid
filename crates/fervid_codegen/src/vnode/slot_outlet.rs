use fervid_core::{
    ElementNode, IntoIdent, SlotOutletCall, SlotOutletFallback, VueImports, fervid_atom,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrayLit, ArrowExpr, ArrowFunctionBody, CallExpr, Callee, Expr, ExprOrSpread, MemberExpr,
        MemberProp, ObjectLit,
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
    pub fn generate_slot_outlet_call(
        &mut self,
        element_node: &ElementNode,
        slot_outlet_call: &SlotOutletCall,
    ) -> Expr {
        // Determine the args length
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
        // First arg: `_ctx.$slots`
        slot_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(Expr::Ident(fervid_atom!("_ctx").into_ident())),
                prop: MemberProp::Ident(fervid_atom!("$slots").into()),
            })),
        });

        // Second arg: slot name (`name="foo"`), slot expression (`:name="foo"`) or "default"
        slot_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(slot_outlet_call.name.to_owned().into()),
        });

        // Third arg (optional): slot props
        if let Some(ref slot_props) = slot_outlet_call.props {
            let props_expr = self.generate_props_expression(slot_props);
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(props_expr),
            });
        } else if capacity > 3 {
            // Pushes `{}` as third argument
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Object(ObjectLit {
                    span: slot_outlet_call.span,
                    props: vec![],
                })),
            });
        }

        // Fourth arg (optional): slot children (fallback)
        if let SlotOutletFallback::UseChildren = slot_outlet_call.fallback {
            let (children, _) = self.generate_element_children(element_node, false);
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
            // () => [child1, child2]
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Arrow(ArrowExpr {
                    span: slot_outlet_call.span,
                    ctxt: Default::default(),
                    params: vec![],
                    body: Box::new(ArrowFunctionBody::Expr(Box::new(children))),
                    is_async: false,
                    is_generator: false,
                    type_params: None,
                    return_type: None,
                })),
            });
        } else if capacity > 4 {
            // Push `undefined` as fallback
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(fervid_atom!("undefined").into_ident())),
            });
        }

        // Fifth arg: noSlotted
        if slot_outlet_call.no_slotted {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(true.into())),
            });
        } else if capacity > 5 {
            // Push `undefined` as fallback
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(fervid_atom!("undefined").into_ident())),
            });
        }

        // Sixth arg: Slot branch key
        if let Some(ref branch_key) = slot_outlet_call.branch_key {
            slot_args.push(ExprOrSpread {
                spread: None,
                expr: branch_key.to_owned(),
            });
        }

        // `renderSlot(_ctx.$slots, "slot-name", { slot: attributes }, () => [slot, children],
        // noSlotted, branchKey)`
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

// Tests live under dedicated tests/ sub-directory
