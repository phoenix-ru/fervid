// Vue reference: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/src/utils.ts#L369-L522
use fervid_core::{
    CallExpression, ExpressionPropNameNode, JsChildNode, ObjectExpression, Property,
    PropsExpression, VueImports, create_call_expression, create_object_expression,
};
use swc_core::common::DUMMY_SP;

use crate::BindingsHelper;

/// Inject a property before user props, below normalizeProps/guardReactiveProps wrappers
pub(crate) fn inject_prop(
    props: &mut Option<PropsExpression>,
    prop: Property,
    bindings_helper: &mut BindingsHelper,
) {
    let Some(current) = props.take() else {
        *props = Some(object_with_prop(prop));
        return;
    };

    *props = Some(inject_into_props(current, prop, bindings_helper).0);
}

/// Check the object under normalization wrappers, matching Vue's injectSlotKey
pub(crate) fn has_unnormalized_prop(props: Option<&PropsExpression>, name: &str) -> bool {
    match props {
        Some(PropsExpression::ObjectExpression(object)) => has_named_prop(object, name),
        Some(PropsExpression::CallExpression(call)) if is_props_wrapper(call.callee) => call
            .arguments
            .first()
            .is_some_and(|first| has_unnormalized_child(first, name)),
        _ => false,
    }
}

fn has_unnormalized_child(child: &JsChildNode, name: &str) -> bool {
    match child {
        JsChildNode::ObjectExpression(object) => has_named_prop(object, name),
        JsChildNode::CallExpression(call) if is_props_wrapper(call.callee) => call
            .arguments
            .first()
            .is_some_and(|first| has_unnormalized_child(first, name)),
        _ => false,
    }
}

fn is_props_wrapper(callee: VueImports) -> bool {
    matches!(
        callee,
        VueImports::NormalizeProps | VueImports::GuardReactiveProps
    )
}

fn prop_name(prop: &Property) -> Option<&str> {
    match &prop.key {
        ExpressionPropNameNode::SimpleExpression(key) => Some(&key.ast.sym),
        ExpressionPropNameNode::CompoundExpression(_) => None,
    }
}

fn has_named_prop(object: &ObjectExpression, name: &str) -> bool {
    object
        .properties
        .iter()
        .any(|existing| prop_name(existing) == Some(name))
}

fn object_with_prop(prop: Property) -> PropsExpression {
    PropsExpression::ObjectExpression(Box::new(create_object_expression(vec![prop], DUMMY_SP)))
}

// The bool indicates a raw expression became mergeProps, allowing its direct guard to be removed
fn inject_into_props(
    props: PropsExpression,
    prop: Property,
    bindings_helper: &mut BindingsHelper,
) -> (PropsExpression, bool) {
    match props {
        PropsExpression::ObjectExpression(mut object) => {
            if !prop_name(&prop).is_some_and(|name| has_named_prop(&object, name)) {
                object.properties.insert(0, prop);
            }
            (PropsExpression::ObjectExpression(object), false)
        }
        PropsExpression::ExpressionNode(expr) => {
            let call = create_call_expression(
                bindings_helper.helper(VueImports::MergeProps),
                vec![
                    object_with_prop(prop).into(),
                    JsChildNode::ExpressionNode(expr),
                ],
                DUMMY_SP,
            );
            (PropsExpression::CallExpression(Box::new(call)), true)
        }
        PropsExpression::CallExpression(mut call) if is_props_wrapper(call.callee) => {
            if !call.arguments.is_empty() {
                let first = call.arguments.remove(0);
                match props_from_child(first) {
                    Ok(inner) => {
                        let (injected, merged_raw) =
                            inject_into_props(inner, prop, bindings_helper);
                        if merged_raw && call.callee == VueImports::GuardReactiveProps {
                            return (injected, false);
                        }
                        call.arguments.insert(0, injected.into());
                        return (PropsExpression::CallExpression(call), false);
                    }
                    Err(first) => call.arguments.insert(0, first),
                }
            }
            inject_into_call(call, prop, bindings_helper)
        }
        PropsExpression::CallExpression(call) => inject_into_call(call, prop, bindings_helper),
    }
}

fn inject_into_call(
    mut call: Box<CallExpression>,
    prop: Property,
    bindings_helper: &mut BindingsHelper,
) -> (PropsExpression, bool) {
    if let Some(JsChildNode::ObjectExpression(first)) = call.arguments.first_mut() {
        if !prop_name(&prop).is_some_and(|name| has_named_prop(first, name)) {
            first.properties.insert(0, prop);
        }
        return (PropsExpression::CallExpression(call), false);
    }

    if call.callee == VueImports::ToHandlers {
        let merged = create_call_expression(
            bindings_helper.helper(VueImports::MergeProps),
            vec![
                object_with_prop(prop).into(),
                JsChildNode::CallExpression(call),
            ],
            DUMMY_SP,
        );
        return (PropsExpression::CallExpression(Box::new(merged)), false);
    }

    call.arguments.insert(0, object_with_prop(prop).into());
    (PropsExpression::CallExpression(call), false)
}

fn props_from_child(child: JsChildNode) -> Result<PropsExpression, JsChildNode> {
    match child {
        JsChildNode::ObjectExpression(object) => Ok(PropsExpression::ObjectExpression(object)),
        JsChildNode::CallExpression(call) => Ok(PropsExpression::CallExpression(call)),
        JsChildNode::ExpressionNode(expr) => Ok(PropsExpression::ExpressionNode(expr)),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/src/utils.ts#L369-L522
    // Keyed-template cases: packages/compiler-core/__tests__/transforms/vFor.spec.ts#L921-L1030
    use fervid_core::{
        ConstantTypes, ExpressionNode, IntoIdent, SimpleExpressionNode, create_object_property,
        create_simple_expression_propname,
    };
    use swc_core::ecma::ast::Expr;

    use super::*;

    #[test]
    fn inserts_key_into_empty_and_object_props() {
        // <template v-for="item in items" :key="item"><div /></template>
        assert_eq!(shape(&injected(None)), "{key:item}");
        // <template v-for="item in items" :key="item"><div :id="id" /></template>
        assert_eq!(
            shape(&injected(Some(object(vec![property("id", "id")])))),
            "{key:item,id:id}"
        );
    }

    #[test]
    fn retains_existing_user_key_in_object_and_merge_props() {
        assert_eq!(
            shape(&injected(Some(object(vec![property("key", "user")])))),
            "{key:user}"
        );
        assert_eq!(
            shape(&injected(Some(call(
                VueImports::MergeProps,
                vec![
                    object(vec![property("key", "user")]).into(),
                    expression("attrs")
                ],
            )))),
            "mergeProps({key:user},attrs)"
        );
    }

    #[test]
    fn prepends_key_to_merged_props_and_first_object() {
        // <template v-for="item in items" :key="item"><div v-bind="attrs" /></template>
        assert_eq!(
            shape(&injected(Some(call(
                VueImports::MergeProps,
                vec![
                    expression("attrs"),
                    object(vec![property("id", "id")]).into()
                ],
            )))),
            "mergeProps({key:item},attrs,{id:id})"
        );
        assert_eq!(
            shape(&injected(Some(call(
                VueImports::MergeProps,
                vec![
                    object(vec![property("id", "id")]).into(),
                    expression("attrs")
                ],
            )))),
            "mergeProps({key:item,id:id},attrs)"
        );
    }

    #[test]
    fn wraps_to_handlers_in_merge_props() {
        // <template v-for="item in items" :key="item"><div v-on="handlers" /></template>
        assert_eq!(
            shape(&injected(Some(call(
                VueImports::ToHandlers,
                vec![expression("handlers")]
            )))),
            "mergeProps({key:item},toHandlers(handlers))"
        );
    }

    #[test]
    fn injects_under_normalization_and_removes_redundant_guard() {
        // <template v-for="item in items" :key="item"><div v-bind="attrs" /></template>
        let guarded = call(VueImports::GuardReactiveProps, vec![expression("attrs")]);
        let normalized = call(VueImports::NormalizeProps, vec![guarded.into()]);
        assert_eq!(
            shape(&injected(Some(normalized))),
            "normalizeProps(mergeProps({key:item},attrs))"
        );
        let normalized_object = call(
            VueImports::NormalizeProps,
            vec![object(vec![property("id", "id")]).into()],
        );
        assert_eq!(
            shape(&injected(Some(normalized_object))),
            "normalizeProps({key:item,id:id})"
        );
    }

    #[test]
    fn merges_raw_binding_and_removes_standalone_guard() {
        assert_eq!(
            shape(&injected(Some(PropsExpression::ExpressionNode(
                match expression("attrs") {
                    JsChildNode::ExpressionNode(expr) => expr,
                    _ => panic!("test expression must be simple"),
                }
            )))),
            "mergeProps({key:item},attrs)"
        );
        assert_eq!(
            shape(&injected(Some(call(
                VueImports::GuardReactiveProps,
                vec![expression("attrs")],
            )))),
            "mergeProps({key:item},attrs)"
        );
    }

    #[test]
    fn detects_user_slot_key_only_in_unnormalized_object() {
        // Vue injectSlotKey checks static object props beneath normalization wrappers
        assert!(has_unnormalized_prop(
            Some(&call(
                VueImports::NormalizeProps,
                vec![
                    call(
                        VueImports::GuardReactiveProps,
                        vec![object(vec![property("key", "user")]).into()],
                    )
                    .into()
                ],
            )),
            "key",
        ));
        assert!(!has_unnormalized_prop(
            Some(&call(
                VueImports::MergeProps,
                vec![
                    object(vec![property("key", "user")]).into(),
                    expression("attrs")
                ],
            )),
            "key",
        ));
        assert!(!has_unnormalized_prop(
            Some(&object(vec![property("id", "id")])),
            "key"
        ));
    }

    fn expression(name: &str) -> JsChildNode {
        JsChildNode::ExpressionNode(Box::new(ExpressionNode::SimpleExpression(
            SimpleExpressionNode {
                ast: Box::new(Expr::Ident(
                    fervid_core::FervidAtom::from(name).into_ident(),
                )),
                is_static: false,
                const_type: ConstantTypes::NotConstant,
                is_handler_key: false,
            },
        )))
    }

    fn property(name: &str, value: &str) -> Property {
        create_object_property(
            create_simple_expression_propname(name.into(), true, DUMMY_SP).into(),
            expression(value),
        )
    }

    fn object(properties: Vec<Property>) -> PropsExpression {
        PropsExpression::ObjectExpression(Box::new(create_object_expression(properties, DUMMY_SP)))
    }

    fn call(callee: VueImports, arguments: Vec<JsChildNode>) -> PropsExpression {
        PropsExpression::CallExpression(Box::new(create_call_expression(
            callee, arguments, DUMMY_SP,
        )))
    }

    // Project typed IR into a readable expression shape; compare argument order and keys
    fn shape(props: &PropsExpression) -> String {
        match props {
            PropsExpression::ObjectExpression(object) => {
                let entries = object
                    .properties
                    .iter()
                    .map(|prop| {
                        format!(
                            "{}:{}",
                            prop_name(prop).unwrap_or("computed"),
                            child_shape(&prop.value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{{{entries}}}")
            }
            PropsExpression::CallExpression(call) => {
                let name = match call.callee {
                    VueImports::MergeProps => "mergeProps",
                    VueImports::NormalizeProps => "normalizeProps",
                    VueImports::GuardReactiveProps => "guardReactiveProps",
                    VueImports::ToHandlers => "toHandlers",
                    _ => panic!("unexpected helper in props"),
                };
                let args = call
                    .arguments
                    .iter()
                    .map(child_shape)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{name}({args})")
            }
            PropsExpression::ExpressionNode(expr) => match expr.as_ref() {
                ExpressionNode::SimpleExpression(simple) => match simple.ast.as_ref() {
                    Expr::Ident(ident) => ident.sym.to_string(),
                    _ => panic!("unexpected expression"),
                },
                _ => panic!("unexpected compound expression"),
            },
        }
    }

    fn child_shape(child: &JsChildNode) -> String {
        match child {
            JsChildNode::ObjectExpression(object) => {
                shape(&PropsExpression::ObjectExpression(object.clone()))
            }
            JsChildNode::CallExpression(call) => {
                shape(&PropsExpression::CallExpression(call.clone()))
            }
            JsChildNode::ExpressionNode(expr) => {
                shape(&PropsExpression::ExpressionNode(expr.clone()))
            }
            _ => panic!("unexpected props child"),
        }
    }

    fn injected(initial: Option<PropsExpression>) -> PropsExpression {
        let mut props = initial;
        inject_prop(
            &mut props,
            property("key", "item"),
            &mut BindingsHelper::default(),
        );
        props.expect("injection always produces props")
    }
}
