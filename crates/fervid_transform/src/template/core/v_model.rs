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
        utils::to_camel_case,
    },
};

pub fn transform_v_model(
    ctx: &mut TransformSfcContext,
    v_model: &VModelDirective,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    Some(transform_v_model_base(ctx, v_model, node)?.result)
}

pub struct VModelTransformState {
    pub result: DirectiveTransformResult,
    pub value: Box<Expr>,
    pub argument: Option<StrOrExpr>,
}

pub fn transform_v_model_base(
    ctx: &mut TransformSfcContext,
    v_model: &VModelDirective,
    node: &ElementNode,
) -> Option<VModelTransformState> {
    let scope = ctx.current_template_scope;
    let expr_span = v_model.value.span();

    // Like vuejs-core `bindingMetadata[rawExp]``, only inspect direct identifiers
    let binding_type = v_model
        .value
        .as_ident()
        .map(|ident| ctx.bindings_helper.get_var_binding_type(scope, &ident.sym));

    let mut incorrect_usage: Option<TemplateErrorKind> = None;

    match binding_type {
        Some(BindingTypes::Props | BindingTypes::PropsAliased) => {
            incorrect_usage = Some(TemplateErrorKind::VModelOnProps);
        }
        Some(BindingTypes::LiteralConst | BindingTypes::SetupConst) => {
            incorrect_usage = Some(TemplateErrorKind::VModelOnConst);
        }
        Some(BindingTypes::TemplateLocal) => {
            incorrect_usage = Some(TemplateErrorKind::VModelOnScopeVariable);
        }
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
        template::directive_transforms::DirectiveTransformResult,
        test_utils::{
            AssertType, element_from_tag, js, js_child_node_to_str, property_to_str, to_str,
        },
    };

    use super::transform_v_model;

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
        transform_v_model(ctx, directive, node)
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

            assert!(transform_v_model(&mut ctx, &directive(value, None, &[]), &node).is_none());
            assert_error(&ctx, expected);
        }

        let mut ctx = TransformSfcContext::anonymous();
        ctx.bindings_helper.template_scopes.push(TemplateScope {
            variables: smallvec![fervid_atom!("item")],
            parent: 0,
        });
        assert!(transform_v_model(&mut ctx, &directive("item", None, &[]), &node).is_none());
        assert_error(&ctx, TemplateErrorKind::VModelOnScopeVariable);
    }
}
