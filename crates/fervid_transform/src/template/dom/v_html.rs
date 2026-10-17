use fervid_core::{
    ConstantTypes, ElementNode, ExpressionNode, ExpressionPropNameNode, JsChildNode, Property,
    SimpleExpressionNode, create_simple_expression_propname, fervid_atom,
};
use swc_core::{common::DUMMY_SP, ecma::ast::Expr};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::directive_transforms::DirectiveTransformResult,
};

pub fn transform_v_html(
    ctx: &mut TransformSfcContext,
    v_html: &Expr,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    if !node.children.is_empty() {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: node.span,
                kind: TemplateErrorKind::VHtmlWithChildren,
            }));
    }

    // TODO Use span of directive
    let span = DUMMY_SP;

    let prop = Property {
        key: ExpressionPropNameNode::SimpleExpression(create_simple_expression_propname(
            fervid_atom!("innerHTML"),
            true,
            span,
        )),
        value: JsChildNode::ExpressionNode(Box::new(ExpressionNode::SimpleExpression(
            SimpleExpressionNode {
                ast: Box::new(v_html.to_owned()),
                // TODO: This is not known - where does vuejs-core parser get this?
                is_static: false,
                const_type: ConstantTypes::NotConstant,
                is_handler_key: false,
            },
        ))),
        span: DUMMY_SP,
    };

    Some(DirectiveTransformResult {
        runtime_directive: None,
        props: vec![prop],
        remove_children: true,
    })
}

#[cfg(test)]
mod tests {
    use fervid_core::{ExpressionNode, JsChildNode, Node, fervid_atom};
    use swc_core::common::DUMMY_SP;

    use crate::{
        TransformSfcContext,
        error::{TemplateErrorKind, TransformError},
        test_utils::{element_from_tag, element_with_children, js, to_str},
    };

    use super::transform_v_html;

    #[test]
    fn it_transforms_v_html_to_inner_html_prop() {
        let mut ctx = TransformSfcContext::anonymous();
        let node = element_from_tag("div");
        let value = js("value");

        let result = transform_v_html(&mut ctx, &value, &node)
            .expect("v-html transform should return a result");

        assert!(result.runtime_directive.is_none());
        assert!(result.remove_children);
        let [prop] = result.props.as_slice() else {
            panic!("v-html should produce one property")
        };
        let fervid_core::ExpressionPropNameNode::SimpleExpression(key) = &prop.key else {
            panic!("innerHTML should use a static property name")
        };
        assert_eq!(key.ast.sym, "innerHTML");
        let JsChildNode::ExpressionNode(value) = &prop.value else {
            panic!("innerHTML should use the directive expression")
        };
        let ExpressionNode::SimpleExpression(value) = value.as_ref() else {
            panic!("v-html value should be a simple expression")
        };
        assert_eq!(to_str(value.ast.as_ref()), "value");
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn it_reports_v_html_with_children() {
        let mut ctx = TransformSfcContext::anonymous();
        let node = element_with_children(vec![Node::Text(fervid_atom!("child"), DUMMY_SP)]);

        let result = transform_v_html(&mut ctx, &js("value"), &node)
            .expect("v-html transform should return a result");

        assert!(result.remove_children);
        assert!(result.runtime_directive.is_none());
        assert!(matches!(
            ctx.errors.as_slice(),
            [TransformError::TemplateError(error)]
                if matches!(error.kind, TemplateErrorKind::VHtmlWithChildren)
        ));
    }
}
