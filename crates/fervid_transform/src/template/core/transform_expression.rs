use fervid_core::{AttributeOrBinding, ElementNode, Node, StrOrExpr};

use crate::{TransformSfcContext, template::expr_transform::BindingsHelperTransform};

pub fn pre_transform_expression(ctx: &mut TransformSfcContext, node: &mut Node) {
    match node {
        Node::Element(element) => transform_element_expressions(ctx, element),
        Node::Interpolation(interpolation) => {
            interpolation.template_scope = ctx.current_template_scope;
            interpolation.patch_flag = ctx
                .bindings_helper
                .transform_expr(&mut interpolation.value, ctx.current_template_scope);
        }
        _ => {}
    }
}

fn transform_element_expressions(ctx: &mut TransformSfcContext, element: &mut ElementNode) {
    // TODO: Sync with https://github.com/vuejs/core/blob/b5f8518379b77c3b62a7a9d2b52f6c76cda09bd5/packages/compiler-core/src/transforms/transformExpression.ts#L56-L90
    let scope = ctx.current_template_scope;

    for prop in &mut element.starting_tag.attributes {
        match prop {
            AttributeOrBinding::VBind(dir) => {
                ctx.bindings_helper.transform_expr(&mut dir.value, scope);

                if let Some(StrOrExpr::Expr(arg)) = &mut dir.argument {
                    ctx.bindings_helper.transform_expr(arg, scope);
                }
            }

            // Do not process exp if this is v-on:arg - we need special handling
            // for wrapping inline statements.
            AttributeOrBinding::VOn(dir) if dir.event.is_none() => {
                if let Some(handler) = &mut dir.handler {
                    ctx.bindings_helper.transform_expr(handler, scope);
                }
            }

            _ => {}
        }
    }
}
