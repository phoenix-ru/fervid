use fervid_core::VueImports;
use swc_core::ecma::ast::Expr;

use crate::{
    TransformSfcContext,
    template::directive_transforms::{BuiltinRuntimeDirective, DirectiveTransformResult},
};

pub fn transform_v_show(
    ctx: &mut TransformSfcContext,
    v_show: &Expr,
) -> Option<DirectiveTransformResult> {
    Some(DirectiveTransformResult {
        runtime_directive: Some(BuiltinRuntimeDirective {
            import: ctx.bindings_helper.helper(VueImports::VShow),
            value: Some(Box::new(v_show.to_owned())),
            arg: None,
            modifiers: vec![],
        }),
        props: vec![],
        remove_children: false,
    })
}

#[cfg(test)]
mod tests {
    use fervid_core::VueImports;

    use crate::{
        TransformSfcContext,
        test_utils::{js, to_str},
    };

    use super::transform_v_show;

    #[test]
    fn it_creates_v_show_runtime_directive() {
        let mut ctx = TransformSfcContext::anonymous();
        let value = js("visible");

        let result =
            transform_v_show(&mut ctx, &value).expect("v-show transform should return a result");

        assert!(result.props.is_empty());
        assert!(!result.remove_children);
        let runtime = result
            .runtime_directive
            .expect("v-show should require a runtime directive");
        assert!(matches!(runtime.import, VueImports::VShow));
        assert_eq!(
            to_str(
                runtime
                    .value
                    .as_deref()
                    .expect("v-show should retain its value")
            ),
            "visible"
        );
        assert!(runtime.arg.is_none());
        assert!(runtime.modifiers.is_empty());
        assert!(ctx.bindings_helper.vue_imports.contains(VueImports::VShow));
    }
}
