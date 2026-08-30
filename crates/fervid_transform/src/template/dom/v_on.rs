use fervid_core::{
    CompoundExpressionPropNameNode, ConstantTypes, ElementNode, ExpressionPropNameNode, FervidAtom,
    JsChildNode, SimpleExpressionNode, VOnDirective, VueImports, create_call_expression,
    create_object_property, create_simple_expression_propname, fervid_atom,
};
use smallvec::{SmallVec, smallvec};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{ArrayLit, BinExpr, ComputedPropName, CondExpr, Expr, Ident, Lit, PropName, Str},
};

use crate::{
    TransformSfcContext,
    template::{
        core::v_on::{finish_v_on, transform_v_on_base},
        directive_transforms::DirectiveTransformResult,
        node_transforms::TransformNodeState,
        utils::{capitalize, maybe_parenthesize},
    },
};

pub fn transform_v_on(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    v_on: &VOnDirective,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    let mut state = transform_v_on_base(ctx, state, v_on, node)?;

    if v_on.modifiers.is_empty() {
        return Some(finish_v_on(state));
    }

    debug_assert_eq!(
        state.result.props.len(),
        1,
        "Base transform should return a single prop"
    );

    let Some(first_prop) = state.result.props.pop() else {
        unreachable!("Base transform should return a single prop")
    };

    let mut key = first_prop.key;
    let mut handler_exp = first_prop.value;

    let ResolveModifiersResult {
        key_modifiers,
        non_key_modifiers,
        event_option_modifiers,
    } = resolve_modifiers(&key, v_on.modifiers.as_slice());

    // Normalize click.right and click.middle since they don't actually fire
    if non_key_modifiers.iter().any(|v| v == "right") {
        key = transform_click(key, fervid_atom!("onContextmenu"));
    }
    if non_key_modifiers.iter().any(|v| v == "middle") {
        key = transform_click(key, fervid_atom!("onMouseup"));
    }

    if !non_key_modifiers.is_empty() {
        handler_exp = JsChildNode::CallExpression(Box::new(create_call_expression(
            VueImports::VOnWithModifiers,
            vec![handler_exp, modifiers_to_js_child_node(non_key_modifiers)],
            DUMMY_SP,
        )));
    }

    if !key_modifiers.is_empty() {
        // If event name is dynamic, always wrap with keys guard
        let wrap = match key {
            ExpressionPropNameNode::SimpleExpression(ref simple) => {
                is_keyboard_event(&simple.ast.sym)
            }
            ExpressionPropNameNode::CompoundExpression(_) => true,
        };

        if wrap {
            handler_exp = JsChildNode::CallExpression(Box::new(create_call_expression(
                VueImports::VOnWithKeys,
                vec![handler_exp, modifiers_to_js_child_node(key_modifiers)],
                DUMMY_SP,
            )));
        }
    }

    if !event_option_modifiers.is_empty() {
        // Capacity here is event_option_modifiers times longest possible modifier (7 -> 8),
        // plus 16 to include potential SimpleExpression ident sym
        let mut postfix = String::with_capacity(16 + 8 * event_option_modifiers.len());
        for modifier in event_option_modifiers {
            capitalize(&modifier, &mut postfix);
        }

        key = match key {
            ExpressionPropNameNode::SimpleExpression(simple) if simple.is_static => {
                postfix.insert_str(0, &simple.ast.sym);
                ExpressionPropNameNode::SimpleExpression(create_simple_expression_propname(
                    FervidAtom::from(postfix),
                    true,
                    DUMMY_SP,
                ))
            }
            ExpressionPropNameNode::SimpleExpression(simple) => {
                ExpressionPropNameNode::CompoundExpression(CompoundExpressionPropNameNode {
                    ast: PropName::Computed(ComputedPropName {
                        span: DUMMY_SP,
                        expr: add_key_with_modifiers(
                            Box::new(Expr::Ident(Ident {
                                span: simple.ast.span,
                                ctxt: Default::default(),
                                sym: simple.ast.sym,
                                optional: false,
                            })),
                            &postfix,
                        ),
                    }),
                    is_handler_key: false,
                })
            }
            ExpressionPropNameNode::CompoundExpression(compound) => {
                let key_expr: Box<Expr> = compound.into();
                ExpressionPropNameNode::CompoundExpression(CompoundExpressionPropNameNode {
                    ast: PropName::Computed(ComputedPropName {
                        span: DUMMY_SP,
                        expr: add_key_with_modifiers(key_expr, &postfix),
                    }),
                    is_handler_key: false,
                })
            }
        };
    }

    state.result.props = vec![create_object_property(key, handler_exp)];

    Some(finish_v_on(state))
}

struct ResolveModifiersResult {
    key_modifiers: SmallVec<[FervidAtom; 1]>,
    non_key_modifiers: SmallVec<[FervidAtom; 1]>,
    event_option_modifiers: SmallVec<[FervidAtom; 1]>,
}

fn resolve_modifiers(
    key: &ExpressionPropNameNode,
    modifiers: &[FervidAtom],
) -> ResolveModifiersResult {
    let mut result = ResolveModifiersResult {
        key_modifiers: smallvec![],
        non_key_modifiers: smallvec![],
        event_option_modifiers: smallvec![],
    };

    for modifier in modifiers {
        if is_event_option_modifier(modifier) {
            result.event_option_modifiers.push(modifier.to_owned());
        } else if maybe_key_modifier(modifier) {
            if let ExpressionPropNameNode::SimpleExpression(simple_expr) = key
                && simple_expr.is_static
            {
                if is_keyboard_event(&simple_expr.ast.sym) {
                    result.key_modifiers.push(modifier.to_owned());
                } else {
                    result.non_key_modifiers.push(modifier.to_owned());
                }
            } else {
                result.key_modifiers.push(modifier.to_owned());
                result.non_key_modifiers.push(modifier.to_owned());
            }
        } else if is_non_key_modifier(modifier) {
            result.non_key_modifiers.push(modifier.to_owned());
        } else {
            result.key_modifiers.push(modifier.to_owned());
        }
    }

    result
}

fn transform_click(key: ExpressionPropNameNode, event: FervidAtom) -> ExpressionPropNameNode {
    match key {
        ExpressionPropNameNode::SimpleExpression(ref simple) => {
            if simple.is_static && simple.ast.sym.eq_ignore_ascii_case("onclick") {
                ExpressionPropNameNode::SimpleExpression(create_simple_expression_propname(
                    event, true, DUMMY_SP,
                ))
            } else {
                key
            }
        }
        // (key) === "onClick" ? "event" : (key)
        ExpressionPropNameNode::CompoundExpression(compound) => {
            let key_expr_paren: Box<Expr> = maybe_parenthesize(compound.into());

            let expr = Box::new(Expr::Cond(CondExpr {
                span: DUMMY_SP,
                test: Box::new(Expr::Bin(BinExpr {
                    span: DUMMY_SP,
                    op: swc_core::ecma::ast::BinaryOp::EqEqEq,
                    left: key_expr_paren.to_owned(),
                    right: Box::new(Expr::Lit(fervid_atom!("onClick").into())),
                })),
                cons: Box::new(Expr::Lit(event.into())),
                alt: key_expr_paren,
            }));

            ExpressionPropNameNode::CompoundExpression(CompoundExpressionPropNameNode {
                ast: PropName::Computed(ComputedPropName {
                    span: DUMMY_SP,
                    expr,
                }),
                is_handler_key: false,
            })
        }
    }
}

// Note: vuejs-core uses a Map based on Object.create(null)
// However, for small sets of elements a plain `matches!`
// significantly out-performs a set (8.4 - 12.2 times faster for the given helpers).
// This also applies to vuejs-core where array.includes is 1.4 times faster than map.

fn is_event_option_modifier(modifier: &FervidAtom) -> bool {
    matches!(modifier.as_str(), "passive" | "once" | "capture")
}
fn is_non_key_modifier(modifier: &FervidAtom) -> bool {
    matches!(
        modifier.as_str(),
        // event propagation management
        "stop" | "prevent" | "self" |
        // system modifiers + exact
        "ctrl" | "shift" | "alt" | "meta" | "exact" |
        // mouse
        "middle"
    )
}
/// left & right could be mouse or key modifiers based on event type
fn maybe_key_modifier(modifier: &FervidAtom) -> bool {
    matches!(modifier.as_str(), "left" | "right")
}
fn is_keyboard_event(modifier: &FervidAtom) -> bool {
    let s = modifier.as_str();
    s.eq_ignore_ascii_case("onkeyup")
        || s.eq_ignore_ascii_case("onkeydown")
        || s.eq_ignore_ascii_case("onkeypress")
}

fn modifiers_to_js_child_node(modifiers: SmallVec<[FervidAtom; 1]>) -> JsChildNode {
    let elems = modifiers
        .into_iter()
        .map(|m| {
            Some(swc_core::ecma::ast::ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(m.into()))),
            })
        })
        .collect();

    JsChildNode::ExpressionNode(Box::new(fervid_core::ExpressionNode::SimpleExpression(
        SimpleExpressionNode {
            ast: Box::new(Expr::Array(ArrayLit {
                span: DUMMY_SP,
                elems,
            })),
            is_static: true,
            const_type: ConstantTypes::CanStringify,
            is_handler_key: false,
        },
    )))
}

/// (key) + "modifiers"
fn add_key_with_modifiers(key: Box<Expr>, modifiers: &str) -> Box<Expr> {
    Box::new(Expr::Bin(BinExpr {
        span: DUMMY_SP,
        op: swc_core::ecma::ast::BinaryOp::Add,
        left: maybe_parenthesize(key),
        right: Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: FervidAtom::from(modifiers),
            raw: None,
        }))),
    }))
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        ExpressionPropNameNode, Node, Property, StrOrExpr, VOnDirective, fervid_atom,
    };
    use swc_core::{common::DUMMY_SP, ecma::ast::PropName};

    use crate::{
        TransformSfcContext,
        template::{
            directive_transforms::DirectiveTransformResult,
            expr_transform::BindingsHelperTransform, node_transforms::TransformNodeState,
        },
        test_utils::{AssertType, element_with_children, js, property_to_str, to_str},
    };

    use super::transform_v_on;

    fn transform(
        event: StrOrExpr,
        modifiers: &[&str],
    ) -> (TransformSfcContext, DirectiveTransformResult) {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let state = TransformNodeState::default();

        let mut directive = VOnDirective {
            event: Some(event),
            handler: Some(js("handler")),
            modifiers: modifiers
                .iter()
                .map(|modifier| (*modifier).into())
                .collect(),
            span: DUMMY_SP,
        };
        if let Some(StrOrExpr::Expr(event)) = directive.event.as_mut() {
            ctx.bindings_helper.transform_expr(event, 0);
        }
        let node = element_with_children(vec![Node::Text(fervid_atom!("child"), DUMMY_SP)]);
        let result = transform_v_on(&mut ctx, &state, &directive, &node)
            .expect("DOM v-on with an event argument should produce a result");
        (ctx, result)
    }

    fn only_property(result: &DirectiveTransformResult) -> &Property {
        assert!(result.runtime_directive.is_none());
        assert!(!result.remove_children);
        let [property] = result.props.as_slice() else {
            panic!("DOM v-on should produce exactly one property")
        };
        assert!(property.key.is_handler_key());
        property
    }

    fn static_key(property: &Property) -> &str {
        let ExpressionPropNameNode::SimpleExpression(key) = &property.key else {
            panic!("static DOM v-on argument should produce a simple property key")
        };
        assert!(key.is_static);
        key.ast.sym.as_ref()
    }

    fn dynamic_key(property: &Property) -> String {
        let ExpressionPropNameNode::CompoundExpression(key) = &property.key else {
            panic!("dynamic DOM v-on argument should produce a compound property key")
        };
        let PropName::Computed(key) = &key.ast else {
            panic!("dynamic DOM v-on argument should produce a computed property name")
        };
        to_str(key.expr.as_ref())
    }

    #[test]
    fn wraps_non_key_modifiers_in_source_order() {
        let (ctx, result) = transform("click".into(), &["stop", "prevent"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onClick");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withModifiers(_ctx.handler,[\"stop\",\"prevent\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn appends_event_options_without_wrapping_handler() {
        let (ctx, result) = transform("click".into(), &["capture", "once"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onClickCaptureOnce");
        assert_eq!(
            property_to_str(property, AssertType::ExpressionNode),
            "_ctx.handler"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn combines_non_key_and_key_guards_for_keyboard_event() {
        let (ctx, result) = transform("keydown".into(), &["stop", "ctrl", "enter"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onKeydown");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_withModifiers(_ctx.handler,[\"stop\",\"ctrl\"]),[\"enter\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn combines_guards_and_event_option_for_keyboard_event() {
        let (ctx, result) = transform("keydown".into(), &["stop", "capture", "ctrl", "a"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onKeydownCapture");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_withModifiers(_ctx.handler,[\"stop\",\"ctrl\"]),[\"a\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn treats_exact_as_non_key_modifier() {
        let (ctx, result) = transform("keyup".into(), &["exact"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onKeyup");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withModifiers(_ctx.handler,[\"exact\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn treats_left_as_key_modifier_for_static_keyboard_event() {
        let (ctx, result) = transform("keyup".into(), &["left"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onKeyup");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_ctx.handler,[\"left\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn applies_both_guards_to_dynamic_left_event() {
        let (ctx, result) = transform(StrOrExpr::Expr(js("event")), &["left"]);
        let property = only_property(&result);

        assert_eq!(dynamic_key(property), "_toHandlerKey(_ctx.event)");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_withModifiers(_ctx.handler,[\"left\"]),[\"left\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn appends_event_options_to_dynamic_event_key() {
        let (ctx, result) = transform(StrOrExpr::Expr(js("event")), &["once", "capture"]);
        let property = only_property(&result);

        assert_eq!(
            dynamic_key(property),
            "_toHandlerKey(_ctx.event)+\"OnceCapture\""
        );
        assert_eq!(
            property_to_str(property, AssertType::ExpressionNode),
            "_ctx.handler"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn remaps_static_right_and_middle_clicks() {
        for (modifier, expected_key) in [("right", "onContextmenu"), ("middle", "onMouseup")] {
            let (ctx, result) = transform("click".into(), &[modifier]);
            let property = only_property(&result);

            assert_eq!(static_key(property), expected_key);
            assert_eq!(
                property_to_str(property, AssertType::CallExpression),
                format!("_withModifiers(_ctx.handler,[\"{modifier}\"])")
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn keeps_static_non_click_right_event_key() {
        let (ctx, result) = transform("mouseup".into(), &["right"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onMouseup");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withModifiers(_ctx.handler,[\"right\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn treats_right_as_key_modifier_for_static_keyboard_event() {
        let (ctx, result) = transform("keyup".into(), &["right"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onKeyup");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_ctx.handler,[\"right\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn right_remap_wins_for_static_click_with_right_and_middle() {
        let (ctx, result) = transform("click".into(), &["right", "middle"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onContextmenu");
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withModifiers(_ctx.handler,[\"right\",\"middle\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn conditionally_remaps_dynamic_right_event_and_applies_both_guards() {
        let (ctx, result) = transform(StrOrExpr::Expr(js("event")), &["right"]);
        let property = only_property(&result);

        assert_eq!(
            dynamic_key(property),
            "_toHandlerKey(_ctx.event)===\"onClick\"?\"onContextmenu\":_toHandlerKey(_ctx.event)"
        );
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_withModifiers(_ctx.handler,[\"right\"]),[\"right\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn conditionally_remaps_dynamic_middle_event_and_applies_non_key_guard() {
        let (ctx, result) = transform(StrOrExpr::Expr(js("event")), &["middle"]);
        let property = only_property(&result);

        assert_eq!(
            dynamic_key(property),
            "_toHandlerKey(_ctx.event)===\"onClick\"?\"onMouseup\":_toHandlerKey(_ctx.event)"
        );
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withModifiers(_ctx.handler,[\"middle\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn conditionally_remaps_dynamic_right_event_before_option_postfix() {
        let (ctx, result) = transform(StrOrExpr::Expr(js("event")), &["right", "once"]);
        let property = only_property(&result);

        assert_eq!(
            dynamic_key(property),
            "(_toHandlerKey(_ctx.event)===\"onClick\"?\"onContextmenu\":_toHandlerKey(_ctx.event))+\"Once\""
        );
        assert_eq!(
            property_to_str(property, AssertType::CallExpression),
            "_withKeys(_withModifiers(_ctx.handler,[\"right\"]),[\"right\"])"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn preserves_event_option_order() {
        let (ctx, result) = transform("click".into(), &["passive", "once", "capture"]);
        let property = only_property(&result);

        assert_eq!(static_key(property), "onClickPassiveOnceCapture");
        assert_eq!(
            property_to_str(property, AssertType::ExpressionNode),
            "_ctx.handler"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn applies_unknown_modifier_only_to_static_keyboard_event() {
        let (keyboard_ctx, keyboard_result) = transform("keyup".into(), &["custom"]);
        let keyboard_property = only_property(&keyboard_result);
        assert_eq!(static_key(keyboard_property), "onKeyup");
        assert_eq!(
            property_to_str(keyboard_property, AssertType::CallExpression),
            "_withKeys(_ctx.handler,[\"custom\"])"
        );
        assert!(keyboard_ctx.errors.is_empty());

        let (click_ctx, click_result) = transform("click".into(), &["custom"]);
        let click_property = only_property(&click_result);
        assert_eq!(static_key(click_property), "onClick");
        assert_eq!(
            property_to_str(click_property, AssertType::ExpressionNode),
            "_ctx.handler"
        );
        assert!(click_ctx.errors.is_empty());
    }
}
