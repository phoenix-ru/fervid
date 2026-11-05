use fervid_core::{
    CallExpression, ConstantTypes, ElementNode, ExpressionNode, ExpressionPropNameNode,
    JsChildNode, Property, SimpleExpressionNode, VueImports, create_simple_expression_propname,
    fervid_atom,
};
use swc_core::{common::DUMMY_SP, ecma::ast::Expr};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::directive_transforms::DirectiveTransformResult,
};

pub fn transform_v_text(
    ctx: &mut TransformSfcContext,
    v_text: &Expr,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    if !node.children.is_empty() {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: node.span,
                kind: TemplateErrorKind::VTextWithChildren,
            }));
    }

    // TODO Use span of directive
    let span = DUMMY_SP;

    // TODO Constant type handling - allow using expr itself when != ConstantTypes::NotConstant
    let value = JsChildNode::CallExpression(Box::new(CallExpression {
        callee: ctx.bindings_helper.helper(VueImports::ToDisplayString),
        span,
        arguments: vec![JsChildNode::ExpressionNode(Box::new(
            ExpressionNode::SimpleExpression(SimpleExpressionNode {
                ast: Box::new(v_text.to_owned()),
                // TODO: This is not known - where does vuejs-core parser get this?
                is_static: false,
                const_type: ConstantTypes::NotConstant,
                is_handler_key: false,
            }),
        ))],
    }));

    let prop = Property {
        key: ExpressionPropNameNode::SimpleExpression(create_simple_expression_propname(
            fervid_atom!("textContent"),
            true,
            span,
        )),
        value,
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
    use fervid_core::{ExpressionNode, JsChildNode, Node, VueImports, fervid_atom};
    use swc_core::common::DUMMY_SP;

    use crate::{
        TransformSfcContext,
        error::{TemplateErrorKind, TransformError},
        test_utils::{element_from_tag, element_with_children, js, to_str},
    };

    use super::transform_v_text;

    #[test]
    fn it_transforms_v_text_to_text_content_prop() {
        let mut ctx = TransformSfcContext::anonymous();
        let node = element_from_tag("div");
        let value = js("value");

        let result = transform_v_text(&mut ctx, &value, &node)
            .expect("v-text transform should return a result");

        assert!(result.runtime_directive.is_none());
        assert!(result.remove_children);
        let [prop] = result.props.as_slice() else {
            panic!("v-text should produce one property")
        };
        let fervid_core::ExpressionPropNameNode::SimpleExpression(key) = &prop.key else {
            panic!("textContent should use a static property name")
        };
        assert_eq!(key.ast.sym, "textContent");
        let JsChildNode::CallExpression(value) = &prop.value else {
            panic!("dynamic v-text should call toDisplayString")
        };
        assert!(matches!(value.callee, VueImports::ToDisplayString));
        let [JsChildNode::ExpressionNode(argument)] = value.arguments.as_slice() else {
            panic!("toDisplayString should receive the directive expression")
        };
        let ExpressionNode::SimpleExpression(argument) = argument.as_ref() else {
            panic!("v-text value should be a simple expression")
        };
        assert_eq!(to_str(argument.ast.as_ref()), "value");
        assert!(
            ctx.bindings_helper
                .vue_imports
                .contains(VueImports::ToDisplayString)
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn it_reports_v_text_with_children() {
        let mut ctx = TransformSfcContext::anonymous();
        let node = element_with_children(vec![Node::Text(fervid_atom!("child"), DUMMY_SP)]);

        let result = transform_v_text(&mut ctx, &js("value"), &node)
            .expect("v-text transform should return a result");

        assert!(result.remove_children);
        assert!(result.runtime_directive.is_none());
        assert!(matches!(
            ctx.errors.as_slice(),
            [TransformError::TemplateError(error)]
                if matches!(error.kind, TemplateErrorKind::VTextWithChildren)
        ));
    }
}
