use fervid_core::{
    BindingTypes, CompoundExpressionNode, CompoundExpressionPropNameNode, ConstantTypes,
    ElementKind, ElementNode, ExpressionNode, ExpressionPropNameNode, FervidAtom, JsChildNode,
    Property, SimpleExpressionNode, StrOrExpr, VModelDirective, create_cache_expression,
    create_simple_expression_propname, fervid_atom, str_to_propname,
};
use swc_core::{
    common::{Span, Spanned},
    ecma::ast::{
        BinExpr, BinaryOp, Bool, ComputedPropName, Expr, KeyValueProp, Lit, ObjectLit, Prop,
        PropName, PropOrSpread, Str,
    },
};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        directive_transforms::DirectiveTransformResult,
        expr_transform::{
            BindingsHelperTransform, is_model_member_expression, transform_v_model_expression,
        },
        node_transforms::TransformNodeState,
        utils::to_camel_case,
    },
};

pub fn transform_v_model(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    v_model: &VModelDirective,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    Some(transform_v_model_base(ctx, state, v_model, node)?.result)
}

pub struct VModelTransformState {
    pub result: DirectiveTransformResult,
    pub value: Box<Expr>,
    pub argument: Option<StrOrExpr>,
}

/// Adapted from https://github.com/vuejs/core/blob/24fccb4ee4139d41df0e395bce96ce7fbb6a50a9/packages/compiler-core/src/transforms/vModel.ts#L23-L156
pub fn transform_v_model_base(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    v_model: &VModelDirective,
    node: &ElementNode,
) -> Option<VModelTransformState> {
    // Note: expression is always defined on `v_model`, guaranteed by the parser

    let scope = state.current_scope;
    let expr_span = v_model.value.span();

    // Check the binding type of the expression.
    // Like vuejs-core `bindingMetadata[rawExp]`, only inspect direct identifiers
    let binding_type = v_model
        .value
        .as_ident()
        .map(|ident| ctx.bindings_helper.get_var_binding_type(scope, &ident.sym));

    // Check incorrect usages
    let mut incorrect_usage: Option<TemplateErrorKind> = None;

    match binding_type {
        // Using v-model on props
        Some(BindingTypes::Props | BindingTypes::PropsAliased) => {
            incorrect_usage = Some(TemplateErrorKind::VModelOnProps);
        }

        // Using v-model on const binding
        Some(BindingTypes::LiteralConst | BindingTypes::SetupConst) => {
            incorrect_usage = Some(TemplateErrorKind::VModelOnConst);
        }

        // Using v-model on a scope variable
        Some(BindingTypes::TemplateLocal) => {
            incorrect_usage = Some(TemplateErrorKind::VModelOnScopeVariable);
        }

        // Using on an incorrect target (e.g. when non-inline mode and not a member expression in
        // vuejs sense - see `is_model_member_expression`)
        Some(
            BindingTypes::SetupLet
            | BindingTypes::SetupRef
            | BindingTypes::SetupMaybeRef
            | BindingTypes::Imported,
        ) => {
            let maybe_ref = ctx.bindings_helper.template_generation_mode.is_inline();

            if !maybe_ref && !is_model_member_expression(&v_model.value) {
                incorrect_usage = Some(TemplateErrorKind::VModelMalformedExpression);
            }
        }
        _ => {}
    }

    if let Some(kind) = incorrect_usage {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: expr_span,
                kind,
            }));
        return None;
    }

    let Some(transformed) =
        transform_v_model_expression(&mut ctx.bindings_helper, &v_model.value, scope)
    else {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: expr_span,
                kind: TemplateErrorKind::VModelMalformedExpression,
            }));
        return None;
    };

    let argument = match &v_model.argument {
        Some(StrOrExpr::Str(argument)) => Some(StrOrExpr::Str(argument.to_owned())),
        Some(StrOrExpr::Expr(argument)) => {
            let mut argument = argument.to_owned();
            ctx.bindings_helper.transform_expr(&mut argument, scope);
            Some(StrOrExpr::Expr(argument))
        }
        None => None,
    };

    let transformed_value = transformed.value;
    let mut props = Vec::with_capacity(
        2 + (!v_model.modifiers.is_empty() && matches!(node.tag_type, ElementKind::Component))
            as usize,
    );

    // modelValue: foo
    props.push(Property {
        key: model_prop_key(argument.as_ref(), v_model.span),
        value: expression_value(transformed_value.to_owned(), ConstantTypes::NotConstant),
        span: v_model.span,
    });

    let mut update_handler = JsChildNode::ExpressionNode(Box::new(
        ExpressionNode::CompoundExpression(CompoundExpressionNode {
            ast: transformed.update_handler,
            is_handler_key: false,
        }),
    ));

    // Cache v-model handler if applicable (when it doesn't refer any scope vars)
    if ctx.cache_handlers && ctx.directive_scopes.v_once == 0 && !transformed.has_template_scope_ref
    {
        update_handler = JsChildNode::CacheExpression(Box::new(create_cache_expression(
            update_handler,
            Default::default(),
        )));
    }

    // "onUpdate:modelValue": $event => (foo = $event)
    props.push(Property {
        key: update_event_key(argument.as_ref(), v_model.span),
        value: update_handler,
        span: v_model.span,
    });

    // modelModifiers: { foo: true, "bar-baz": true }
    if !v_model.modifiers.is_empty() && matches!(node.tag_type, ElementKind::Component) {
        props.push(Property {
            key: modifiers_key(argument.as_ref(), v_model.span),
            value: modifiers_value(&v_model.modifiers, v_model.span),
            span: v_model.span,
        });
    }

    Some(VModelTransformState {
        result: DirectiveTransformResult {
            runtime_directive: None,
            props,
            remove_children: false,
        },
        value: transformed_value,
        argument,
    })
}

fn model_prop_key(argument: Option<&StrOrExpr>, span: Span) -> ExpressionPropNameNode {
    match argument {
        Some(StrOrExpr::Str(argument)) => {
            create_simple_expression_propname(argument.value.to_owned(), true, span).into()
        }
        Some(StrOrExpr::Expr(argument)) => dynamic_key(argument.to_owned(), span),
        None => create_simple_expression_propname(fervid_atom!("modelValue"), true, span).into(),
    }
}

fn update_event_key(argument: Option<&StrOrExpr>, span: Span) -> ExpressionPropNameNode {
    match argument {
        Some(StrOrExpr::Str(argument)) => {
            let mut name = String::with_capacity("onUpdate:".len() + argument.value.len());
            name.push_str("onUpdate:");
            to_camel_case(&argument.value, &mut name);

            ExpressionPropNameNode::from(create_simple_expression_propname(
                FervidAtom::from(name),
                true,
                span,
            ))
        }

        Some(StrOrExpr::Expr(argument)) => {
            let expression = Box::new(Expr::Bin(BinExpr {
                span,
                op: BinaryOp::Add,
                left: Box::new(Expr::Lit(Lit::Str(Str {
                    span,
                    value: fervid_atom!("onUpdate:"),
                    raw: None,
                }))),
                right: argument.to_owned(),
            }));

            dynamic_key(expression, span)
        }

        None => ExpressionPropNameNode::from(create_simple_expression_propname(
            fervid_atom!("onUpdate:modelValue"),
            true,
            span,
        )),
    }
}

fn modifiers_key(argument: Option<&StrOrExpr>, span: Span) -> ExpressionPropNameNode {
    match argument {
        Some(StrOrExpr::Str(argument)) => {
            let mut name = String::with_capacity(argument.value.len() + "Modifiers".len());
            name.push_str(&argument.value);
            name.push_str("Modifiers");

            create_simple_expression_propname(FervidAtom::from(name), true, span).into()
        }

        Some(StrOrExpr::Expr(argument)) => {
            let expression = Box::new(Expr::Bin(BinExpr {
                span,
                op: BinaryOp::Add,
                left: argument.to_owned(),
                right: Box::new(Expr::Lit(Lit::Str(Str {
                    span,
                    value: fervid_atom!("Modifiers"),
                    raw: None,
                }))),
            }));

            dynamic_key(expression, span)
        }

        None => {
            create_simple_expression_propname(fervid_atom!("modelModifiers"), true, span).into()
        }
    }
}

fn dynamic_key(expression: Box<Expr>, span: Span) -> ExpressionPropNameNode {
    ExpressionPropNameNode::CompoundExpression(CompoundExpressionPropNameNode {
        ast: PropName::Computed(ComputedPropName {
            span,
            expr: expression,
        }),
        is_handler_key: false,
    })
}

fn expression_value(expression: Box<Expr>, const_type: ConstantTypes) -> JsChildNode {
    JsChildNode::ExpressionNode(Box::new(ExpressionNode::SimpleExpression(
        SimpleExpressionNode {
            ast: expression,
            is_static: false,
            const_type,
            is_handler_key: false,
        },
    )))
}

fn modifiers_value(modifiers: &[FervidAtom], span: Span) -> JsChildNode {
    let properties = modifiers
        .iter()
        .map(|modifier| {
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: str_to_propname(modifier, span),
                value: Box::new(Expr::Lit(Lit::Bool(Bool { span, value: true }))),
            })))
        })
        .collect();

    expression_value(
        Box::new(Expr::Object(ObjectLit {
            span,
            props: properties,
        })),
        ConstantTypes::CanCache,
    )
}

#[cfg(test)]
mod tests {
    // Adapted from packages/compiler-core/__tests__/transforms/vModel.spec.ts in vuejs-core
    use fervid_core::{
        BindingTypes, ConstantTypes, ElementKind, ExpressionNode, ExpressionPropNameNode,
        JsChildNode, Property, StrOrExpr, TemplateGenerationMode, VModelDirective, fervid_atom,
    };
    use smallvec::smallvec;
    use swc_core::{common::DUMMY_SP, ecma::ast::PropName};

    use crate::{
        SetupBinding, TemplateScope, TransformSfcContext,
        error::{TemplateErrorKind, TransformError},
        template::{
            core::v_model::transform_v_model, directive_transforms::DirectiveTransformResult,
            node_transforms::TransformNodeState,
        },
        test_utils::{
            AssertType, element_from_tag, js, js_child_node_to_str, property_to_str, to_str,
        },
    };

    #[test]
    fn it_transforms_v_model() {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let node = element_from_tag("input");
        let helper = &mut ctx.bindings_helper;

        // const Ref = ref('foo')
        // const MaybeRef = useSomething()
        // const Const = 123
        // let Let = 123
        // const Reactive = reactive({})
        helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("Ref"),
            BindingTypes::SetupRef,
        ));
        helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("MaybeRef"),
            BindingTypes::SetupMaybeRef,
        ));
        helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("Const"),
            BindingTypes::SetupConst,
        ));
        helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("Let"),
            BindingTypes::SetupLet,
        ));
        helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("Reactive"),
            BindingTypes::SetupReactiveConst,
        ));

        macro_rules! test {
            ($value: literal, $expected_value: literal, $expected_handler: literal) => {
                let result = transform(&mut ctx, &directive($value, None, &[]), &node);
                let [model, update] = result.props.as_slice() else {
                    panic!("core v-model should produce value and update properties")
                };
                assert_eq!(
                    property_to_str(model, AssertType::ExpressionNode),
                    $expected_value,
                    "model value for {}",
                    $value
                );
                assert_eq!(
                    property_to_str(update, AssertType::ExpressionNode),
                    $expected_handler,
                    "update handler for {}",
                    $value
                );
                assert!(ctx.errors.is_empty(), "unexpected errors for {}", $value);
            };
            ($value: literal, $expected_error: expr) => {
                assert!(
                    transform_v_model(
                        &mut ctx,
                        &TransformNodeState::default(),
                        &directive($value, None, &[]),
                        &node,
                    )
                    .is_none(),
                    "read-only target {} should not produce model props",
                    $value
                );
                assert_error(&ctx, $expected_error);
                ctx.errors.clear();
            };
        }

        // Syntax is like that:
        // first element is `$expr` in `v-model="$expr"`;
        // second is the transformed value;
        // third is the transformed update handler.

        // DEV DIRECT
        test!("Ref", "$setup.Ref", "$event=>$setup.Ref=$event");
        test!(
            "MaybeRef",
            "$setup.MaybeRef",
            "$event=>$setup.MaybeRef=$event"
        );
        // Adaptation from the expression-only test: core transform rejects const targets
        test!("Const", TemplateErrorKind::VModelOnConst);
        test!("Let", "$setup.Let", "$event=>$setup.Let=$event");
        test!(
            "Reactive",
            "$setup.Reactive",
            "$event=>$setup.Reactive=$event"
        ); // TODO must err
        test!("Unknown", "_ctx.Unknown", "$event=>_ctx.Unknown=$event");

        // DEV INDIRECT
        test!("Ref.x", "$setup.Ref.x", "$event=>$setup.Ref.x=$event");
        test!(
            "MaybeRef.x",
            "$setup.MaybeRef.x",
            "$event=>$setup.MaybeRef.x=$event"
        );
        test!("Const.x", "$setup.Const.x", "$event=>$setup.Const.x=$event");
        test!("Let.x", "$setup.Let.x", "$event=>$setup.Let.x=$event");
        test!(
            "Reactive.x",
            "$setup.Reactive.x",
            "$event=>$setup.Reactive.x=$event"
        );
        test!(
            "Unknown.x",
            "_ctx.Unknown.x",
            "$event=>_ctx.Unknown.x=$event"
        );

        // PROD DIRECT
        ctx.bindings_helper.is_prod = true;
        ctx.bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
        test!("Ref", "Ref.value", "$event=>Ref.value=$event");
        test!(
            "MaybeRef",
            "_unref(MaybeRef)",
            // This is the official spec, but it is inconsistent with the `v-on` transform
            "$event=>_isRef(MaybeRef)?MaybeRef.value=$event:null"
        );
        test!("Const", TemplateErrorKind::VModelOnConst);
        test!(
            "Let",
            "_unref(Let)",
            "$event=>_isRef(Let)?Let.value=$event:Let=$event"
        );
        test!("Reactive", "Reactive", "$event=>Reactive=$event"); // TODO must err
        test!("Unknown", "_ctx.Unknown", "$event=>_ctx.Unknown=$event");

        // PROD INDIRECT
        test!("Ref.x", "Ref.value.x", "$event=>Ref.value.x=$event");
        test!(
            "MaybeRef.x",
            "_unref(MaybeRef).x",
            "$event=>_unref(MaybeRef).x=$event"
        );
        test!("Const.x", "Const.x", "$event=>Const.x=$event");
        test!("Let.x", "_unref(Let).x", "$event=>_unref(Let).x=$event");
        test!("Reactive.x", "Reactive.x", "$event=>Reactive.x=$event");
        test!(
            "Unknown.x",
            "_ctx.Unknown.x",
            "$event=>_ctx.Unknown.x=$event"
        );
    }

    #[test]
    fn it_transforms_v_model_arg() {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let node = element_from_tag("input");

        // const Ref = ref('foo')
        ctx.bindings_helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("Ref"),
            BindingTypes::SetupRef,
        ));

        macro_rules! test {
            ($argument: literal, $expected: literal) => {
                let result = transform(
                    &mut ctx,
                    &directive("dummy", Some(StrOrExpr::Expr(js($argument))), &[]),
                    &node,
                );
                let [model, _] = result.props.as_slice() else {
                    panic!("core v-model should produce value and update properties")
                };
                assert_eq!(dynamic_key(model), $expected, "argument {}", $argument);
                assert!(ctx.errors.is_empty(), "unexpected errors for {}", $argument);
            };
        }

        // The first is `$expr` in `v-model:[$expr]="dummy"`, the second is expected

        // DEV
        test!("Ref", "$setup.Ref");
        test!("Unknown", "_ctx.Unknown");
        test!("\"string\"", "\"string\"");

        // PROD
        ctx.bindings_helper.is_prod = true;
        ctx.bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
        test!("Ref", "Ref.value");
        test!("Unknown", "_ctx.Unknown");
        test!("\"string\"", "\"string\"");
    }

    #[test]
    fn transforms_model_values_and_assignment_handlers() {
        let node = element_from_tag("input");

        for (value, expected_value, expected_handler) in [
            ("model", "_ctx.model", "$event=>_ctx.model=$event"),
            (
                "model[index]",
                "_ctx.model[_ctx.index]",
                "$event=>_ctx.model[_ctx.index]=$event",
            ),
            ("变.量", "_ctx.变.量", "$event=>_ctx.变.量=$event"),
        ] {
            let mut ctx = TransformSfcContext::anonymous();
            ctx.cache_handlers = false;
            let result = transform(&mut ctx, &directive(value, None, &[]), &node);
            let [model, update] = result.props.as_slice() else {
                panic!("core v-model should produce value and update properties")
            };

            assert_eq!(static_key(model), "modelValue");
            assert_eq!(static_key(update), "onUpdate:modelValue");
            assert_eq!(
                property_to_str(model, AssertType::ExpressionNode),
                expected_value
            );
            assert_eq!(
                property_to_str(update, AssertType::ExpressionNode),
                expected_handler
            );
            assert!(result.runtime_directive.is_none());
            assert!(!result.remove_children);
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn transforms_static_and_dynamic_arguments() {
        let node = element_from_tag("input");

        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let result = transform(
            &mut ctx,
            &directive("model", Some("foo-value".into()), &[]),
            &node,
        );
        let [model, update] = result.props.as_slice() else {
            panic!("core v-model should produce value and update properties")
        };
        assert_eq!(static_key(model), "foo-value");
        assert_eq!(static_key(update), "onUpdate:fooValue");

        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let result = transform(
            &mut ctx,
            &directive("model", Some(StrOrExpr::Expr(js("name"))), &[]),
            &node,
        );
        let [model, update] = result.props.as_slice() else {
            panic!("core v-model should produce value and update properties")
        };
        assert_eq!(dynamic_key(model), "_ctx.name");
        assert_eq!(dynamic_key(update), "\"onUpdate:\"+_ctx.name");
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn adds_cacheable_modifiers_only_for_components() {
        let mut component = element_from_tag("Comp");
        component.tag_type = ElementKind::Component;

        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let result = transform(
            &mut ctx,
            &directive("model", None, &["trim", "bar-baz"]),
            &component,
        );
        let [_, _, modifiers] = result.props.as_slice() else {
            panic!("component v-model modifiers should produce a third property")
        };
        assert_eq!(static_key(modifiers), "modelModifiers");
        assert_eq!(
            property_to_str(modifiers, AssertType::ExpressionNode),
            "{trim:true,\"bar-baz\":true}"
        );
        let JsChildNode::ExpressionNode(value) = &modifiers.value else {
            panic!("model modifiers should use an expression value")
        };
        let ExpressionNode::SimpleExpression(value) = value.as_ref() else {
            panic!("model modifiers should use a simple expression")
        };
        assert!(matches!(value.const_type, ConstantTypes::CanCache));

        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let result = transform(
            &mut ctx,
            &directive("model", None, &["trim"]),
            &element_from_tag("input"),
        );
        assert_eq!(result.props.len(), 2);
    }

    #[test]
    fn transforms_inline_ref_assignments() {
        let node = element_from_tag("input");

        for (name, binding_type, expected_value, expected_handler) in [
            (
                "Ref",
                BindingTypes::SetupRef,
                "Ref.value",
                "$event=>Ref.value=$event",
            ),
            (
                "MaybeRef",
                BindingTypes::SetupMaybeRef,
                "_unref(MaybeRef)",
                "$event=>_isRef(MaybeRef)?MaybeRef.value=$event:null",
            ),
            (
                "Let",
                BindingTypes::SetupLet,
                "_unref(Let)",
                "$event=>_isRef(Let)?Let.value=$event:Let=$event",
            ),
        ] {
            let mut ctx = TransformSfcContext::anonymous();
            ctx.cache_handlers = false;
            ctx.bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
            ctx.bindings_helper
                .setup_bindings
                .push(SetupBinding::new(name.into(), binding_type));

            let result = transform(&mut ctx, &directive(name, None, &[]), &node);
            assert_eq!(
                property_to_str(&result.props[0], AssertType::ExpressionNode),
                expected_value
            );
            assert_eq!(
                property_to_str(&result.props[1], AssertType::ExpressionNode),
                expected_handler
            );
        }
    }

    #[test]
    fn caches_only_scope_independent_handlers() {
        let node = element_from_tag("input");
        let mut ctx = TransformSfcContext::anonymous();
        let result = transform(&mut ctx, &directive("model", None, &[]), &node);
        assert_eq!(
            js_child_node_to_str(&result.props[1].value),
            "cache($event=>_ctx.model=$event)"
        );

        let mut ctx = TransformSfcContext::anonymous();
        ctx.directive_scopes.v_once = 1;
        let result = transform(&mut ctx, &directive("model", None, &[]), &node);
        assert!(matches!(
            result.props[1].value,
            JsChildNode::ExpressionNode(_)
        ));

        let mut ctx = TransformSfcContext::anonymous();
        ctx.bindings_helper.template_scopes.push(TemplateScope {
            variables: smallvec![fervid_atom!("i")],
            parent: 0,
        });
        let result = transform(&mut ctx, &directive("model[i]", None, &[]), &node);
        assert!(matches!(
            result.props[1].value,
            JsChildNode::ExpressionNode(_)
        ));
        assert_eq!(
            property_to_str(&result.props[0], AssertType::ExpressionNode),
            "_ctx.model[i]"
        );
    }

    #[test]
    fn reports_invalid_or_read_only_targets() {
        let node = element_from_tag("input");

        for (value, binding_type, expected) in [
            ("a+b", None, TemplateErrorKind::VModelMalformedExpression),
            (
                "undefined",
                None,
                TemplateErrorKind::VModelMalformedExpression,
            ),
            (
                "prop",
                Some(BindingTypes::Props),
                TemplateErrorKind::VModelOnProps,
            ),
            (
                "constant",
                Some(BindingTypes::SetupConst),
                TemplateErrorKind::VModelOnConst,
            ),
        ] {
            let mut ctx = TransformSfcContext::anonymous();
            if let Some(binding_type) = binding_type {
                ctx.bindings_helper
                    .setup_bindings
                    .push(SetupBinding::new(value.into(), binding_type));
            }
            let state = TransformNodeState::default();

            assert!(
                transform_v_model(&mut ctx, &state, &directive(value, None, &[]), &node).is_none()
            );
            assert_error(&ctx, expected);
        }

        let mut ctx = TransformSfcContext::anonymous();
        ctx.bindings_helper.template_scopes.push(TemplateScope {
            variables: smallvec![fervid_atom!("item")],
            parent: 0,
        });
        let state = TransformNodeState::default();
        assert!(
            transform_v_model(&mut ctx, &state, &directive("item", None, &[]), &node).is_none()
        );
        assert_error(&ctx, TemplateErrorKind::VModelOnScopeVariable);
    }

    fn directive(value: &str, argument: Option<StrOrExpr>, modifiers: &[&str]) -> VModelDirective {
        VModelDirective {
            argument,
            value: js(value),
            update_handler: None,
            modifiers: modifiers
                .iter()
                .map(|modifier| (*modifier).into())
                .collect(),
            span: DUMMY_SP,
        }
    }

    fn transform(
        ctx: &mut TransformSfcContext,
        directive: &VModelDirective,
        node: &fervid_core::ElementNode,
    ) -> DirectiveTransformResult {
        let state = TransformNodeState::default();
        transform_v_model(ctx, &state, directive, node)
            .expect("v-model with a valid expression should produce a result")
    }

    fn static_key(property: &Property) -> &str {
        let ExpressionPropNameNode::SimpleExpression(key) = &property.key else {
            panic!("static v-model key should use a simple expression")
        };
        assert!(key.is_static);
        key.ast.sym.as_ref()
    }

    fn dynamic_key(property: &Property) -> String {
        let ExpressionPropNameNode::CompoundExpression(key) = &property.key else {
            panic!("dynamic v-model key should use a compound expression")
        };
        let PropName::Computed(key) = &key.ast else {
            panic!("dynamic v-model key should use a computed property name")
        };
        to_str(key.expr.as_ref())
    }

    fn assert_error(ctx: &TransformSfcContext, expected: TemplateErrorKind) {
        let [TransformError::TemplateError(error)] = ctx.errors.as_slice() else {
            panic!("expected exactly one template error")
        };
        assert_eq!(
            std::mem::discriminant(&error.kind),
            std::mem::discriminant(&expected)
        );
    }
}
