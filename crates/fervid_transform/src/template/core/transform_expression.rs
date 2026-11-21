use fervid_core::{AttributeOrBinding, ElementNode, Node, StrOrExpr};

use crate::{
    TransformSfcContext,
    template::{
        expr_transform::BindingsHelperTransform, node_transforms::TransformNodeState,
        resolutions::maybe_resolve_directive,
    },
};

pub fn pre_transform_expression(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    node: &mut Node,
) {
    match node {
        Node::Element(element) => transform_element_expressions(ctx, state, element),
        Node::Interpolation(interpolation) => {
            interpolation.template_scope = state.current_scope;
            interpolation.patch_flag = ctx
                .bindings_helper
                .transform_expr(&mut interpolation.value, state.current_scope)
                .has_js_bindings;
        }
        _ => {}
    }
}

fn transform_element_expressions(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    element: &mut ElementNode,
) {
    // TODO: Sync with https://github.com/vuejs/core/blob/b5f8518379b77c3b62a7a9d2b52f6c76cda09bd5/packages/compiler-core/src/transforms/transformExpression.ts#L56-L90
    let scope_to_use = state.current_scope;

    for prop in &mut element.starting_tag.attributes {
        match prop {
            AttributeOrBinding::VBind(dir) => {
                dir.const_type = ctx
                    .bindings_helper
                    .transform_expr(&mut dir.value, scope_to_use)
                    .const_type;

                if let Some(StrOrExpr::Expr(arg)) = &mut dir.argument {
                    ctx.bindings_helper.transform_expr(arg, scope_to_use);
                }
            }

            AttributeOrBinding::VOn(dir) => {
                if let Some(StrOrExpr::Expr(event_expr)) = dir.event.as_mut() {
                    ctx.bindings_helper.transform_expr(event_expr, scope_to_use);
                }

                // Do not process exp if this is v-on:arg - we need special handling
                // for wrapping inline statements.
                if dir.event.is_none()
                    && let Some(handler) = &mut dir.handler
                {
                    ctx.bindings_helper.transform_expr(handler, scope_to_use);
                }
            }

            _ => {}
        }
    }

    if let Some(ref mut directives) = element.starting_tag.directives {
        macro_rules! maybe_transform {
            ($key: ident) => {
                if let Some(ref mut expr) = directives.$key {
                    ctx.bindings_helper.transform_expr(expr, scope_to_use);
                }
            };
        }
        maybe_transform!(v_html);
        maybe_transform!(v_memo);
        maybe_transform!(v_show);
        maybe_transform!(v_text);

        if let Some(v_slot) = directives.v_slot.as_mut() {
            // Transform `v-slot` argument if it is dynamic
            if let Some(StrOrExpr::Expr(ref mut slot_name_expr)) = v_slot.slot_name {
                ctx.bindings_helper
                    .transform_expr(slot_name_expr, scope_to_use);
            }
            if let Some(ref mut v_slot_value) = v_slot.value {
                ctx.bindings_helper
                    .transform_pat(v_slot_value, scope_to_use);
            }
        }

        // Transform custom directives
        for custom_directive in directives.custom.iter_mut() {
            if let Some(ref mut value) = custom_directive.value {
                ctx.bindings_helper.transform_expr(value, scope_to_use);
            }
            if let Some(StrOrExpr::Expr(ref mut argument)) = custom_directive.argument {
                ctx.bindings_helper.transform_expr(argument, scope_to_use);
            }

            // Try resolving it
            maybe_resolve_directive(ctx, &custom_directive.name, scope_to_use);
        }
    }
}
