use fervid_core::{AttributeOrBinding, ElementNode, Node, StrOrExpr};

use crate::{
    TransformSfcContext,
    template::{expr_transform::BindingsHelperTransform, resolutions::maybe_resolve_directive},
};

pub fn pre_transform_expression(ctx: &mut TransformSfcContext, node: &mut Node) {
    match node {
        Node::Element(element) => transform_element_expressions(ctx, element),
        Node::Interpolation(interpolation) => {
            interpolation.template_scope = ctx.current_template_scope;
            interpolation.patch_flag = ctx
                .bindings_helper
                .transform_expr(&mut interpolation.value, ctx.current_template_scope)
                .has_js_bindings;
        }
        _ => {}
    }
}

fn transform_element_expressions(ctx: &mut TransformSfcContext, element: &mut ElementNode) {
    // TODO: Sync with https://github.com/vuejs/core/blob/b5f8518379b77c3b62a7a9d2b52f6c76cda09bd5/packages/compiler-core/src/transforms/transformExpression.ts#L56-L90
    let scope_to_use = ctx.current_template_scope;

    for prop in &mut element.starting_tag.attributes {
        match prop {
            AttributeOrBinding::VBind(dir) => {
                ctx.bindings_helper
                    .transform_expr(&mut dir.value, scope_to_use);

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

        // TODO: This needs to be considered together with v-model transform
        // for v_model in directives.v_model.iter_mut() {
        //     ctx.bindings_helper
        //         .transform_v_model(v_model, scope_to_use, patch_hints);
        // }

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
