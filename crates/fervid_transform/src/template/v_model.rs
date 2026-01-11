use fervid_core::{BindingTypes, ElementNode, VModelDirective};
use swc_core::common::Spanned;

use crate::{
    error::{TemplateError, TemplateErrorKind, TransformError},
    TransformSfcContext,
};

// Adapted from https://github.com/vuejs/core/blob/24fccb4ee4139d41df0e395bce96ce7fbb6a50a9/packages/compiler-core/src/transforms/vModel.ts#L23-L156
pub fn transform_model(
    ctx: &mut TransformSfcContext,
    v_model: &mut VModelDirective,
    _element_node: &ElementNode,
    scope_to_use: u32,
) {
    // Note: expression is always defined on `v_model`, guaranteed by the parser

    // Check the binding type of the expression
    let v_model_ident = v_model.value.as_ident();
    let v_model_value_span = v_model.value.span();
    let binding_type = v_model_ident.and_then(|v| {
        // TODO: Use binding metadata
        ctx.bindings_helper
            .setup_bindings
            .iter()
            .find(|it| it.sym == v.sym)
            .map(|v| v.binding_type)
    });

    // Using v-model on props
    if matches!(
        binding_type,
        Some(BindingTypes::Props | BindingTypes::PropsAliased)
    ) {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                kind: TemplateErrorKind::VModelOnProps,
                span: v_model_value_span,
            }));
        return;
    }

    let maybe_ref = ctx.bindings_helper.template_generation_mode.is_inline()
        && matches!(
            binding_type,
            Some(BindingTypes::SetupLet | BindingTypes::SetupRef | BindingTypes::SetupMaybeRef)
        );

    // Using a malformed expression
    if !v_model.value.is_member() && !maybe_ref {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: v_model_value_span,
                kind: TemplateErrorKind::VModelMalformedExpression,
            }));
        return;
    }

    // Using v-model on a scope variable
    if v_model_ident.is_some_and(|ident| {
        matches!(
            ctx.bindings_helper
                .find_in_template_scopes(scope_to_use, &ident.sym),
            BindingTypes::TemplateLocal,
        )
    }) {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: v_model_value_span,
                kind: TemplateErrorKind::VModelOnScopeVariable,
            }));
        return;
    }

    todo!("finish the checks")
}
