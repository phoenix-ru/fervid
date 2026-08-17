use std::borrow::Cow;

use fervid_core::{
    BindingTypes, CompoundExpressionNode, CompoundExpressionPropNameNode, ConstantTypes,
    ElementKind, ElementNode, ExpressionNode, ExpressionPropNameNode, FervidAtom, IntoIdent,
    JsChildNode, Property, SimpleExpressionPropNameNode, StrOrExpr, VOnDirective, VueImports,
    create_cache_expression,
};
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{
        ArrowExpr, BlockStmt, BlockStmtOrExpr, CallExpr, Callee, ComputedPropName, Expr,
        ExprOrSpread, IdentName, OptChainBase, PropName,
    },
};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        directive_transforms::DirectiveTransformResult,
        expr_transform::BindingsHelperTransform,
        utils::{to_pascal_case, wrap_in_event_arrow},
        v_on::wrap_in_args_arrow,
    },
    utils::unwrap_ts_node_expr,
};

pub fn transform_v_on(
    ctx: &mut TransformSfcContext,
    v_on: &VOnDirective,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    let state = transform_v_on_base(ctx, v_on, node)?;
    Some(finish_v_on(state))
}

pub struct VOnTransformState {
    pub result: DirectiveTransformResult,
    pub should_cache: bool,
}

pub fn transform_v_on_base(
    ctx: &mut TransformSfcContext,
    v_on: &VOnDirective,
    node: &ElementNode,
) -> Option<VOnTransformState> {
    let Some(ref v_on_arg) = v_on.event else {
        // v-on without arg is handled directly in ./transformElement.ts due to its affecting
        // codegen for the entire props object. This transform here is only for v-on
        // *with* args.
        return None;
    };

    if v_on.handler.is_none() && v_on.modifiers.is_empty() {
        ctx.errors
            .push(crate::error::TransformError::TemplateError(TemplateError {
                kind: TemplateErrorKind::VOnNoExpression,
                span: v_on.span,
            }))
    }

    let event_name = match v_on_arg {
        StrOrExpr::Str(s) => static_event_key(ctx, node, &s.value, s.span),
        StrOrExpr::Expr(expr) => dynamic_event_key(ctx, expr, v_on.span),
    };

    let (handler, should_cache) = transform_handler(ctx, v_on, node);

    let property = Property {
        key: event_name,
        value: JsChildNode::ExpressionNode(Box::new(ExpressionNode::CompoundExpression(
            CompoundExpressionNode {
                ast: handler,
                is_handler_key: false,
            },
        ))),
        span: v_on.span,
    };

    Some(VOnTransformState {
        result: DirectiveTransformResult {
            runtime_directive: None,
            props: vec![property],
            remove_children: false,
        },
        should_cache,
    })
}

pub fn finish_v_on(mut state: VOnTransformState) -> DirectiveTransformResult {
    let should_cache = state.should_cache;

    state.result.props = state
        .result
        .props
        .into_iter()
        .map(|mut property| {
            property.key.set_handler_key(true);

            if should_cache {
                property.value = wrap_with_cache_expr(property.value);
            }
            property
        })
        .collect();

    state.result
}

fn transform_handler(
    ctx: &mut TransformSfcContext,
    v_on: &VOnDirective,
    node: &ElementNode,
) -> (Box<Expr>, bool) {
    let Some(handler) = v_on.handler.as_ref() else {
        let should_cache = ctx.cache_handlers && ctx.directive_scopes.v_once == 0;
        return (empty_handler(), should_cache);
    };

    let scope_to_use = ctx.current_template_scope;

    let handler_kind = classify_handler(handler);
    let is_member_expr = matches!(handler_kind, HandlerKind::MemberExpr);
    let is_inline = matches!(handler_kind, HandlerKind::Inline);

    // TODO: This must use `exp.constType` instead
    let is_runtime_constant = is_runtime_constant_handler(ctx, handler, scope_to_use);

    // Note: This is a difference with vuejs-core - we wrap the handler in arrow function first
    // before transforming so that the transformer picks up the `$event` correctly.
    // This might break the inner spans, however.
    let mut handler = if is_inline {
        wrap_in_event_arrow(handler.to_owned())
    } else {
        handler.to_owned()
    };

    let has_scope_ref = ctx
        .bindings_helper
        .transform_expr(&mut handler, scope_to_use)
        .has_template_scope_ref;

    let should_cache = ctx.cache_handlers &&
        // unnecessary to cache inside v-once
        ctx.directive_scopes.v_once == 0 &&
        // runtime constants don't need to be cached
        // (this is analyzed by compileScript in SFC <script setup>)
        !is_runtime_constant &&
        // https://github.com/vuejs/core/issues/1541 bail if this is a member exp handler passed to a component -
        // we need to use the original function to preserve arity,
        // e.g. <transition> relies on checking cb.length to determine
        // transition end handling. Inline function is ok since its arity
        // is preserved even when cached.
        !(is_member_expr && matches!(node.tag_type, ElementKind::Component)) &&
        // bail if the function references closure variables (v-for, v-slot)
        // it must be passed fresh to avoid stale values.
        !has_scope_ref;

    if should_cache && is_member_expr {
        handler = wrap_in_args_arrow(handler, true);
    }

    (handler, should_cache)
}

enum HandlerKind {
    MemberExpr,
    Inline,
    Function,
}

fn classify_handler(handler: &Expr) -> HandlerKind {
    let unwrapped = unwrap_ts_node_expr(handler);
    match unwrapped {
        Expr::Member(_) => HandlerKind::MemberExpr,
        Expr::OptChain(chain) if matches!(chain.base.as_ref(), OptChainBase::Member(_)) => {
            HandlerKind::MemberExpr
        }
        Expr::Ident(ident) if ident.sym != "undefined" => HandlerKind::MemberExpr,
        Expr::Fn(_) | Expr::Arrow(_) => HandlerKind::Function,
        _ => HandlerKind::Inline,
    }
}

fn is_runtime_constant_handler(
    ctx: &mut TransformSfcContext,
    handler: &Expr,
    scope_to_use: u32,
) -> bool {
    let Expr::Ident(identifier) = unwrap_ts_node_or_paren_expr(handler) else {
        return false;
    };

    let binding_type = ctx
        .bindings_helper
        .get_var_binding_type(scope_to_use, &identifier.sym);
    matches!(
        binding_type,
        BindingTypes::SetupConst | BindingTypes::LiteralConst | BindingTypes::SetupReactiveConst
    )
}

/// This is an iterative copy of `unwrap_ts_node` but with Expr::Paren unwrapping
fn unwrap_ts_node_or_paren_expr(mut expr: &Expr) -> &Expr {
    loop {
        expr = match expr {
            Expr::Paren(paren) => &paren.expr,
            Expr::TsConstAssertion(ts_const_assertion) => &ts_const_assertion.expr,
            Expr::TsNonNull(ts_non_null_expr) => &ts_non_null_expr.expr,
            Expr::TsAs(ts_as_expr) => &ts_as_expr.expr,
            Expr::TsInstantiation(ts_instantiation) => &ts_instantiation.expr,
            Expr::TsSatisfies(ts_satisfies_expr) => &ts_satisfies_expr.expr,
            _ => return expr,
        }
    }
}

fn static_event_key(
    ctx: &mut TransformSfcContext,
    node: &ElementNode,
    raw_name: &FervidAtom,
    raw_name_span: Span,
) -> ExpressionPropNameNode {
    let mut raw_name = Cow::Borrowed(raw_name.as_str());

    if !ctx.bindings_helper.is_prod && raw_name.starts_with("vnode") {
        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span: raw_name_span,
                kind: TemplateErrorKind::VNodeHooks,
            }));
    }
    if let Some(rest) = raw_name.strip_prefix("vue:") {
        raw_name = Cow::Owned(format!("vnode-{rest}"))
    }

    let wrap_with_to_handler_key = !matches!(node.tag_type, ElementKind::Element)
        || raw_name.starts_with("vnode")
        || !raw_name.chars().any(|c| c.is_ascii_uppercase());

    let event_string = if wrap_with_to_handler_key {
        let mut out = String::with_capacity(2 + raw_name.len());
        out.push_str("on");
        to_pascal_case(&raw_name, &mut out);
        out
    } else {
        format!("on:{raw_name}")
    };

    ExpressionPropNameNode::SimpleExpression(SimpleExpressionPropNameNode {
        ast: IdentName {
            span: raw_name_span,
            sym: FervidAtom::from(event_string),
        },
        is_static: true,
        const_type: ConstantTypes::CanStringify,
        is_handler_key: false,
    })
}

fn dynamic_event_key(
    ctx: &mut TransformSfcContext,
    event: &Expr,
    span: Span,
) -> ExpressionPropNameNode {
    let call = Box::new(Expr::Call(CallExpr {
        span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(
            ctx.bindings_helper
                .helper(VueImports::ToHandlerKey)
                .as_atom()
                .into_ident(),
        ))),
        args: vec![ExprOrSpread {
            spread: None,
            expr: Box::new(event.to_owned()),
        }],
        type_args: None,
    }));

    ExpressionPropNameNode::CompoundExpression(CompoundExpressionPropNameNode {
        ast: PropName::Computed(ComputedPropName { span, expr: call }),
        is_handler_key: false,
    })
}

fn empty_handler() -> Box<Expr> {
    Box::new(Expr::Arrow(ArrowExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        params: vec![],
        body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
            span: DUMMY_SP,
            ctxt: Default::default(),
            stmts: vec![],
        })),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    }))
}

fn wrap_with_cache_expr(value: JsChildNode) -> JsChildNode {
    JsChildNode::CacheExpression(Box::new(create_cache_expression(value, Default::default())))
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        ElementKind, ExpressionPropNameNode, Node, Property, StrOrExpr, VOnDirective, VueImports,
        fervid_atom,
    };
    use swc_core::{common::DUMMY_SP, ecma::ast::PropName};

    use crate::{
        TransformSfcContext,
        error::{TemplateErrorKind, TransformError},
        template::{
            directive_transforms::DirectiveTransformResult, expr_transform::BindingsHelperTransform,
        },
        test_utils::{
            AssertType, element_from_tag, element_with_children, js, property_to_str, to_str,
        },
    };

    use super::transform_v_on;

    /*
     * TODO tests when cache IR/options/scope analysis exist:
     * - element member handler caches latest-value wrapper
     * - component member handler remains uncached to preserve arity
     * - inline statement handler caches
     * - inline function handler caches as a whole
     * - modifier-wrapped handler caches after DOM augmentation
     * - key-modifier handler caches after withKeys augmentation
     * - empty modifier-only handler caches
     * - handler inside v-once remains uncached
     * - handler referencing v-for alias remains uncached
     * - handler referencing v-slot prop remains uncached
     * - Unicode template-scope alias remains uncached
     * - direct setup/runtime constant remains uncached
     * - cached keyup preserves NEED_HYDRATION but does not add PROPS
     * - cached click has no unnecessary patch flag
     */

    fn transform(
        event: StrOrExpr,
        handler: Option<&str>,
        modifiers: &[&str],
        node: &fervid_core::ElementNode,
    ) -> (TransformSfcContext, DirectiveTransformResult) {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let mut directive = VOnDirective {
            event: Some(event),
            handler: handler.map(js),
            modifiers: modifiers
                .iter()
                .map(|modifier| (*modifier).into())
                .collect(),
            span: DUMMY_SP,
        };
        if let Some(StrOrExpr::Expr(event)) = directive.event.as_mut() {
            ctx.bindings_helper.transform_expr(event, 0);
        }
        let result = transform_v_on(&mut ctx, &directive, node)
            .expect("v-on with an event argument should produce a result");
        (ctx, result)
    }

    fn only_property(result: &DirectiveTransformResult) -> &Property {
        assert!(result.runtime_directive.is_none());
        assert!(!result.remove_children);
        let [property] = result.props.as_slice() else {
            panic!("v-on should produce exactly one property")
        };
        assert!(property.key.is_handler_key());
        property
    }

    fn static_key(property: &Property) -> &str {
        let ExpressionPropNameNode::SimpleExpression(key) = &property.key else {
            panic!("static v-on argument should produce a simple property key")
        };
        assert!(key.is_static);
        key.ast.sym.as_ref()
    }

    #[test]
    fn transforms_static_event_names_and_member_handlers() {
        let node = element_from_tag("div");

        for (event, expected_key, expression, expected_handler) in [
            ("click", "onClick", "handler", "_ctx.handler"),
            (
                "foo-bar",
                "onFooBar",
                "object.handler",
                "_ctx.object.handler",
            ),
        ] {
            let (ctx, result) = transform(event.into(), Some(expression), &[], &node);
            let property = only_property(&result);
            assert_eq!(static_key(property), expected_key);
            assert_eq!(
                property_to_str(property, AssertType::ExpressionNode),
                expected_handler
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn handles_uppercase_native_and_component_events() {
        let element = element_from_tag("div");
        let mut component = element_from_tag("Comp");
        component.tag_type = ElementKind::Component;

        for (node, expected_key) in [(&element, "on:Foo"), (&component, "onFoo")] {
            let (ctx, result) = transform("Foo".into(), Some("handler"), &[], node);
            let property = only_property(&result);
            assert_eq!(static_key(property), expected_key);
            assert_eq!(
                property_to_str(property, AssertType::ExpressionNode),
                "_ctx.handler"
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn handles_camelcase_update_event_on_element_and_component() {
        let element = element_from_tag("div");
        let mut component = element_from_tag("Comp");
        component.tag_type = ElementKind::Component;

        for (node, expected_key) in [
            (&element, "on:update:modelValue"),
            (&component, "onUpdate:modelValue"),
        ] {
            let (ctx, result) = transform("update:modelValue".into(), Some("handler"), &[], node);
            let property = only_property(&result);
            assert_eq!(static_key(property), expected_key);
            assert_eq!(
                property_to_str(property, AssertType::ExpressionNode),
                "_ctx.handler"
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn transforms_vnode_hook_names_and_reports_deprecated_raw_name() {
        let node = element_from_tag("div");

        for (event, expected_key) in [
            ("vue:mounted", "onVnodeMounted"),
            ("vue:before-update", "onVnodeBeforeUpdate"),
        ] {
            let (ctx, result) = transform(event.into(), Some("handler"), &[], &node);
            assert_eq!(static_key(only_property(&result)), expected_key);
            assert!(ctx.errors.is_empty());
        }

        let (ctx, result) = transform("vnode-mounted".into(), Some("handler"), &[], &node);
        assert_eq!(static_key(only_property(&result)), "onVnodeMounted");
        assert!(matches!(
            ctx.errors.as_slice(),
            [TransformError::TemplateError(error)]
                if matches!(error.kind, TemplateErrorKind::VNodeHooks)
        ));
    }

    #[test]
    fn transforms_dynamic_event_and_registers_helper() {
        let node = element_from_tag("div");
        let (ctx, result) = transform(StrOrExpr::Expr(js("event")), Some("handler"), &[], &node);
        let property = only_property(&result);
        let ExpressionPropNameNode::CompoundExpression(key) = &property.key else {
            panic!("dynamic v-on argument should produce a computed property key")
        };
        let PropName::Computed(key) = &key.ast else {
            panic!("dynamic v-on argument should produce a computed property name")
        };
        assert_eq!(to_str(key.expr.as_ref()), "_toHandlerKey(_ctx.event)");
        assert_eq!(
            property_to_str(property, AssertType::ExpressionNode),
            "_ctx.handler"
        );
        assert!(
            ctx.bindings_helper
                .vue_imports
                .contains(VueImports::ToHandlerKey)
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn transforms_complex_dynamic_event_expression_inside_helper() {
        let node = element_from_tag("div");
        let (ctx, result) = transform(
            StrOrExpr::Expr(js("event(foo)")),
            Some("handler"),
            &[],
            &node,
        );
        let property = only_property(&result);
        let ExpressionPropNameNode::CompoundExpression(key) = &property.key else {
            panic!("dynamic v-on argument should produce a computed property key")
        };
        let PropName::Computed(key) = &key.ast else {
            panic!("dynamic v-on argument should produce a computed property name")
        };
        assert_eq!(
            to_str(key.expr.as_ref()),
            "_toHandlerKey(_ctx.event(_ctx.foo))"
        );
        assert_eq!(
            property_to_str(property, AssertType::ExpressionNode),
            "_ctx.handler"
        );
        assert!(
            ctx.bindings_helper
                .vue_imports
                .contains(VueImports::ToHandlerKey)
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn preserves_computed_member_handler_and_transforms_computed_key() {
        let node = element_from_tag("div");
        let (ctx, result) = transform("click".into(), Some("a['b' + c]"), &[], &node);

        assert_eq!(
            property_to_str(only_property(&result), AssertType::ExpressionNode),
            "_ctx.a[\"b\"+_ctx.c]"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn preserves_optional_member_handler_without_extra_wrapper() {
        let node = element_from_tag("div");
        let (ctx, result) = transform("click".into(), Some("foo?.bar"), &[], &node);

        assert_eq!(
            property_to_str(only_property(&result), AssertType::ExpressionNode),
            "_ctx.foo?.bar"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn wraps_inline_handlers_in_event_arrow() {
        let node = element_from_tag("div");

        for (expression, expected) in [
            ("foo()", "$event=>_ctx.foo()"),
            ("count++", "$event=>_ctx.count++"),
            ("foo($event)", "$event=>_ctx.foo($event)"),
        ] {
            let (ctx, result) = transform("click".into(), Some(expression), &[], &node);
            assert_eq!(
                property_to_str(only_property(&result), AssertType::ExpressionNode),
                expected
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn preserves_function_handlers_while_transforming_their_bodies() {
        let node = element_from_tag("div");

        for (expression, expected) in [
            ("event => foo(event)", "event=>_ctx.foo(event)"),
            (
                "function (event) { foo(event) }",
                "function(event){_ctx.foo(event);}",
            ),
        ] {
            let (ctx, result) = transform("click".into(), Some(expression), &[], &node);
            assert_eq!(
                property_to_str(only_property(&result), AssertType::ExpressionNode),
                expected
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn preserves_async_function_handlers_while_transforming_their_bodies() {
        let node = element_from_tag("div");

        for (expression, expected) in [
            ("async event => foo(event)", "async event=>_ctx.foo(event)"),
            (
                "async function (event) { foo(event) }",
                "async function(event){_ctx.foo(event);}",
            ),
        ] {
            let (ctx, result) = transform("click".into(), Some(expression), &[], &node);
            assert_eq!(
                property_to_str(only_property(&result), AssertType::ExpressionNode),
                expected
            );
            assert!(ctx.errors.is_empty());
        }
    }

    #[test]
    fn uses_empty_handler_for_modifier_only_event_without_error() {
        let node = element_with_children(vec![Node::Text(fervid_atom!("child"), DUMMY_SP)]);
        let (ctx, result) = transform("click".into(), None, &["stop"], &node);

        assert_eq!(
            property_to_str(only_property(&result), AssertType::ExpressionNode),
            "()=>{}"
        );
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn reports_missing_handler_and_still_uses_empty_handler() {
        let node = element_from_tag("div");
        let (ctx, result) = transform("click".into(), None, &[], &node);

        assert_eq!(
            property_to_str(only_property(&result), AssertType::ExpressionNode),
            "()=>{}"
        );
        assert!(matches!(
            ctx.errors.as_slice(),
            [TransformError::TemplateError(error)]
                if matches!(error.kind, TemplateErrorKind::VOnNoExpression)
        ));
    }
}
