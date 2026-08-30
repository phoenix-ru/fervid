use fervid_core::{
    AttributeOrBinding, ElementKind, ElementNode, ExpressionPropNameNode, StrOrExpr,
    VBindDirective, VModelDirective, VueImports,
};
use swc_core::common::Spanned;

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        core::v_model::{VModelTransformState, transform_v_model_base},
        directive_transforms::{BuiltinRuntimeDirective, DirectiveTransformResult},
        node_transforms::TransformNodeState,
    },
};

pub fn transform_v_model(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    v_model: &VModelDirective,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    let VModelTransformState {
        result: mut base_result,
        value: transformed_value,
        argument,
    } = transform_v_model_base(ctx, state, v_model, node)?;

    // Base transform has errors OR component v-model (only need props).
    // Note: Fervid doesn't normally return empty `props`,
    // but this check is still beneficial.
    // Note: Fervid additionally distinguishes built-ins.
    if base_result.props.is_empty()
        || matches!(
            node.tag_type,
            ElementKind::Component | ElementKind::Builtin(_)
        )
    {
        return Some(base_result);
    }

    if let Some(ref arg) = v_model.argument {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: arg.span(),
                kind: TemplateErrorKind::VModelArgOnElement,
            }));
    }

    // TODO: Implement when custom elements are supported
    let is_custom_element = false;
    let dev = !ctx.bindings_helper.is_prod;

    let tag = node.starting_tag.tag_name.as_str();
    if matches!(tag, "input" | "textarea" | "select") || is_custom_element {
        let mut directive_to_use = VueImports::VModelText;
        let mut is_invalid_type = false;

        if tag == "input" || is_custom_element {
            let mut found = false;

            for attr in node.starting_tag.attributes.iter() {
                match attr {
                    // type="something"
                    AttributeOrBinding::RegularAttribute { name, value, span }
                        if name == "type" =>
                    {
                        match value.as_str() {
                            "radio" => directive_to_use = VueImports::VModelRadio,
                            "checkbox" => directive_to_use = VueImports::VModelCheckbox,
                            "file" => {
                                is_invalid_type = true;
                                ctx.errors
                                    .push(TransformError::TemplateError(TemplateError {
                                        span: v_model.span,
                                        kind: TemplateErrorKind::VModelOnFileInputElement,
                                    }));
                            }
                            // Text type.
                            // Note: vuejs-core ignores any other types and considers them "text"
                            _ if dev => {
                                check_duplicate_value(ctx, node);
                            }
                            _ => {}
                        }
                        found = true;
                        break;
                    }

                    // :type="foo"
                    AttributeOrBinding::VBind(VBindDirective {
                        argument: Some(StrOrExpr::Str(s)),
                        ..
                    }) if s.value == "type" => {
                        found = true;
                        directive_to_use = VueImports::VModelDynamic;
                        break;
                    }

                    // :[foo]="bar"
                    // Element has bindings with dynamic keys, which can possibly contain "type".
                    AttributeOrBinding::VBind(VBindDirective {
                        argument: None | Some(StrOrExpr::Expr(_)),
                        ..
                    }) => {
                        found = true;
                        directive_to_use = VueImports::VModelDynamic;
                        // Don't break here and try to find a narrower match
                    }

                    _ => {}
                }
            }

            if dev && !found {
                check_duplicate_value(ctx, node);
            }
        } else if tag == "select" {
            directive_to_use = VueImports::VModelSelect;
        } else if dev {
            // <textarea>
            check_duplicate_value(ctx, node);
        }

        // Inject runtime directive by returning the helper symbol.
        // The import will replace a `resolveDirective` call.
        if !is_invalid_type {
            base_result.runtime_directive = Some(BuiltinRuntimeDirective {
                import: ctx.bindings_helper.helper(directive_to_use),
                value: Some(transformed_value),
                arg: argument,
                modifiers: v_model.modifiers.to_owned(),
            });
        }
    } else {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: v_model.span,
                kind: TemplateErrorKind::VModelOnInvalidElement,
            }));
    }

    // Native v-model doesn't need the `modelValue` props since they are also
    // passed to the runtime as `binding.value`. Removing it reduces code size.
    base_result.props.retain(|prop| !matches!(prop.key, ExpressionPropNameNode::SimpleExpression(ref s) if s.ast.sym == "modelValue"));

    Some(base_result)
}

fn check_duplicate_value(ctx: &mut TransformSfcContext, node: &ElementNode) {
    let value_attr = node
        .starting_tag
        .attributes
        .iter()
        .find_map(|attr| match attr {
            AttributeOrBinding::VBind(v_bind) => match v_bind.argument {
                Some(StrOrExpr::Str(ref s)) if s.value == "value" => Some(s),
                _ => None,
            },
            _ => None,
        });

    if let Some(value) = value_attr {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: value.span,
                kind: TemplateErrorKind::VModelUnnecessaryValue,
            }));
    }
}

#[cfg(test)]
mod tests {
    // Adapted from packages/compiler-dom/__tests__/transforms/vModel.spec.ts in vuejs-core
    use fervid_core::{
        AttributeOrBinding, BindingTypes, ExpressionPropNameNode, StrOrExpr, VBindDirective,
        VModelDirective, VueImports, fervid_atom,
    };
    use swc_core::common::DUMMY_SP;

    use crate::{
        SetupBinding, TransformSfcContext,
        error::{TemplateErrorKind, TransformError},
        template::{
            directive_transforms::DirectiveTransformResult, node_transforms::TransformNodeState,
        },
        test_utils::{element_from_tag, js, to_str},
    };

    use super::transform_v_model;

    fn regular(name: &str, value: &str) -> AttributeOrBinding {
        AttributeOrBinding::RegularAttribute {
            name: name.into(),
            value: value.into(),
            span: DUMMY_SP,
        }
    }

    fn bind(argument: Option<StrOrExpr>, value: &str) -> AttributeOrBinding {
        AttributeOrBinding::VBind(VBindDirective {
            argument,
            value: js(value),
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: DUMMY_SP,
        })
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
        tag: &str,
        attributes: Vec<AttributeOrBinding>,
        argument: Option<StrOrExpr>,
        modifiers: &[&str],
    ) -> (TransformSfcContext, DirectiveTransformResult) {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let mut node = element_from_tag(tag);
        node.starting_tag.attributes = attributes;
        let state = TransformNodeState::default();

        let result = transform_v_model(
            &mut ctx,
            &state,
            &directive("model", argument, modifiers),
            &node,
        )
        .expect("v-model with a valid expression should produce a result");
        (ctx, result)
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
    fn selects_runtime_directive_for_native_controls() {
        for (tag, attributes, expected) in [
            ("input", vec![], VueImports::VModelText),
            (
                "input",
                vec![regular("type", "text")],
                VueImports::VModelText,
            ),
            (
                "input",
                vec![regular("type", "radio")],
                VueImports::VModelRadio,
            ),
            (
                "input",
                vec![regular("type", "checkbox")],
                VueImports::VModelCheckbox,
            ),
            ("textarea", vec![], VueImports::VModelText),
            ("select", vec![], VueImports::VModelSelect),
        ] {
            let (ctx, result) = transform(tag, attributes, None, &[]);
            let runtime = result
                .runtime_directive
                .expect("native v-model should require a runtime directive");

            assert_eq!(runtime.import, expected);
            assert!(ctx.bindings_helper.vue_imports.contains(expected));
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn selects_dynamic_runtime_for_bound_input_type() {
        let (ctx, result) = transform(
            "input",
            vec![bind(Some("type".into()), "inputType")],
            None,
            &[],
        );
        let runtime = result
            .runtime_directive
            .expect("bound input type should use a runtime-selected directive");

        assert_eq!(runtime.import, VueImports::VModelDynamic);
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn uses_processed_value_and_removes_native_model_value_prop() {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        ctx.bindings_helper.template_generation_mode = fervid_core::TemplateGenerationMode::Inline;
        ctx.bindings_helper.setup_bindings.push(SetupBinding::new(
            fervid_atom!("inputModel"),
            BindingTypes::SetupRef,
        ));
        let node = element_from_tag("input");
        let state = TransformNodeState::default();

        let result = transform_v_model(
            &mut ctx,
            &state,
            &directive("inputModel", None, &["lazy"]),
            &node,
        )
        .expect("input v-model should produce a result");
        let runtime = result
            .runtime_directive
            .as_ref()
            .expect("input v-model should require a runtime directive");

        assert_eq!(
            to_str(
                runtime
                    .value
                    .as_deref()
                    .expect("runtime directive should retain processed model value")
            ),
            "inputModel.value"
        );
        assert_eq!(runtime.modifiers, vec![fervid_atom!("lazy")]);
        assert!(runtime.arg.is_none());

        let [update] = result.props.as_slice() else {
            panic!("native v-model should retain only its update handler")
        };
        let ExpressionPropNameNode::SimpleExpression(key) = &update.key else {
            panic!("model update handler should use a static key")
        };
        assert_eq!(key.ast.sym, "onUpdate:modelValue");
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn reports_native_argument_but_preserves_runtime_argument() {
        let (ctx, result) = transform("input", vec![], Some("value".into()), &[]);
        assert_error(&ctx, TemplateErrorKind::VModelArgOnElement);

        let runtime = result
            .runtime_directive
            .expect("native v-model should still receive a runtime directive");
        let Some(StrOrExpr::Str(argument)) = runtime.arg else {
            panic!("runtime directive should preserve static model argument")
        };
        assert_eq!(argument.value, "value");
        assert_eq!(result.props.len(), 2);
    }

    #[test]
    fn reports_file_input_and_invalid_element() {
        let (ctx, result) = transform("input", vec![regular("type", "file")], None, &[]);
        assert_error(&ctx, TemplateErrorKind::VModelOnFileInputElement);
        assert!(result.runtime_directive.is_none());

        let (ctx, result) = transform("span", vec![], None, &[]);
        assert_error(&ctx, TemplateErrorKind::VModelOnInvalidElement);
        assert!(result.runtime_directive.is_none());
    }

    #[test]
    fn reports_bound_value_for_text_controls_but_allows_static_value() {
        let (ctx, _) = transform(
            "input",
            vec![regular("type", "text"), bind(Some("value".into()), "model")],
            None,
            &[],
        );
        assert_error(&ctx, TemplateErrorKind::VModelUnnecessaryValue);

        let (ctx, _) = transform(
            "input",
            vec![regular("type", "text"), regular("value", "model")],
            None,
            &[],
        );
        assert!(ctx.errors.is_empty());
    }
}
