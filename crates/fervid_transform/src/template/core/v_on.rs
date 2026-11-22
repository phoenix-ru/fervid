use std::borrow::Cow;

use fervid_core::{
    BindingTypes, CompoundExpressionNode, CompoundExpressionPropNameNode, ConstantTypes,
    ElementKind, ElementNode, ExpressionNode, ExpressionPropNameNode, FervidAtom, IntoIdent,
    JsChildNode, Property, SimpleExpressionPropNameNode, StrOrExpr, VOnDirective, VueImports,
    create_cache_expression, fervid_atom,
};
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{
        ArrowExpr, ArrowFunctionBody, BinExpr, BinaryOp, BindingIdent, CallExpr, Callee,
        ComputedPropName, Expr, ExprOrSpread, FunctionBody, IdentName, OptChainBase, Pat, PropName,
        RestPat,
    },
};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        directive_transforms::DirectiveTransformResult,
        expr_transform::BindingsHelperTransform,
        node_transforms::TransformNodeState,
        utils::{to_pascal_case, wrap_in_event_arrow},
    },
};

pub fn transform_v_on(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    v_on: &VOnDirective,
    node: &ElementNode,
) -> Option<DirectiveTransformResult> {
    let v_on_state = transform_v_on_base(ctx, state, v_on, node)?;
    Some(finish_v_on(v_on_state))
}

pub struct VOnTransformState {
    pub result: DirectiveTransformResult,
    pub should_cache: bool,
}

pub fn transform_v_on_base(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
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

    let (handler, should_cache) = transform_handler(ctx, state, v_on, node);

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
    state: &TransformNodeState,
    v_on: &VOnDirective,
    node: &ElementNode,
) -> (Box<Expr>, bool) {
    let Some(handler) = v_on.handler.as_ref() else {
        let should_cache = ctx.cache_handlers && ctx.directive_scopes.v_once == 0;
        return (empty_handler(), should_cache);
    };

    let scope_to_use = state.current_scope;

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
        !(is_member_expr && matches!(node.tag_type, ElementKind::Component | ElementKind::Builtin(_))) &&
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
    let unwrapped = unwrap_ts_node_or_paren_expr(handler);
    match unwrapped {
        // This is always as-is
        Expr::Fn(_) | Expr::Arrow(_) => HandlerKind::Function,

        // This is getting `(...args) => _ctx.smth && _ctx.smth(...args)`
        Expr::Member(_) => HandlerKind::MemberExpr,
        Expr::Ident(ident) if ident.sym != "undefined" => HandlerKind::MemberExpr,
        Expr::OptChain(chain) if matches!(chain.base.as_ref(), OptChainBase::Member(_)) => {
            HandlerKind::MemberExpr
        }

        // This is getting `$event =>`
        _ => HandlerKind::Inline,
        // Error? This would definitely lead to a runtime error
        // Expr::SuperProp(_)
        // | Expr::Yield(_)
        // | Expr::MetaProp(_)
        // | Expr::Await(_)
        // | Expr::JSXMember(_)
        // | Expr::JSXNamespacedName(_)
        // | Expr::JSXEmpty(_)
        // | Expr::JSXElement(_)
        // | Expr::JSXFragment(_)
        // | Expr::TsTypeAssertion(_)
        // | Expr::TsConstAssertion(_)
        // | Expr::TsAs(_)
        // | Expr::TsInstantiation(_)
        // | Expr::TsSatisfies(_)
        // | Expr::PrivateName(_)
        // | Expr::Invalid(_) |
    }
}

fn is_runtime_constant_handler(
    ctx: &mut TransformSfcContext,
    handler: &Expr,
    scope_to_use: u32,
) -> bool {
    let unwrapped = unwrap_ts_node_or_paren_expr(handler);
    let Expr::Ident(identifier) = unwrapped else {
        // Literals are runtime constants
        return matches!(unwrapped, Expr::Lit(_));
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

    // `vue:` events have special transform
    // `vue:update` -> `onVnodeUpdate`
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

/// Wraps dynamic key in `_toHandlerKey`
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
        body: Box::new(ArrowFunctionBody::FunctionBody(FunctionBody {
            span: DUMMY_SP,
            stmts: vec![],
        })),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    }))
}

/// Wraps in `(...args) => _ctx.smth && _ctx.smth(...args)`.
///
/// `needs_check` signifies if `&&` check is needed
fn wrap_in_args_arrow(mut expr: Box<Expr>, needs_check: bool) -> Box<Expr> {
    let check = if needs_check {
        Some(expr.to_owned())
    } else {
        None
    };

    let args_ident = fervid_atom!("args").into_ident();

    // Modify to call expression
    expr = Box::new(Expr::Call(CallExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        callee: Callee::Expr(expr),
        args: vec![ExprOrSpread {
            spread: Some(DUMMY_SP),
            expr: Box::new(Expr::Ident(args_ident.to_owned())),
        }],
        type_args: None,
    }));

    // Add a check if needed
    if let Some(check_expr) = check {
        expr = Box::new(Expr::Bin(BinExpr {
            span: DUMMY_SP,
            op: BinaryOp::LogicalAnd,
            left: check_expr,
            right: expr,
        }));
    }

    // ...args
    let args_pat = Pat::Rest(RestPat {
        span: DUMMY_SP,
        dot3_token: DUMMY_SP,
        arg: Box::new(Pat::Ident(BindingIdent {
            id: args_ident,
            type_ann: None,
        })),
        type_ann: None,
    });

    // Return arrow
    Box::new(Expr::Arrow(ArrowExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        params: vec![args_pat],
        body: Box::new(ArrowFunctionBody::Expr(expr)),
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
        BindingTypes, ElementKind, ExpressionPropNameNode, Node, Property, StrOrExpr,
        TemplateGenerationMode, VOnDirective, VueImports, fervid_atom,
    };
    use swc_core::{common::DUMMY_SP, ecma::ast::PropName};

    use crate::{
        SetupBinding, TransformSfcContext,
        error::{TemplateErrorKind, TransformError},
        template::{
            directive_transforms::DirectiveTransformResult,
            expr_transform::BindingsHelperTransform, node_transforms::TransformNodeState,
        },
        test_utils::{
            AssertType, element_from_tag, element_with_children, js, property_to_str, to_str, ts,
        },
    };

    use super::{transform_v_on, transform_v_on_base};

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

    #[test]
    fn it_transforms_static_event() {
        macro_rules! test {
            ($from: literal, $to: literal) => {{
                assert_eq!(static_event_name($from), $to);
            }};
        }

        test!("update:model-value", "onUpdate:modelValue");
        test!("update:modelValue", "on:update:modelValue");
        test!("update!model-value", "onUpdate!modelValue");
        test!("update!!model-value", "onUpdate!!modelValue");
        test!("update-model-value", "onUpdateModelValue");
        test!("vue:update", "onVnodeUpdate");
        test!("vue:updateFoo", "onVnodeUpdateFoo");
        test!("click", "onClick");
        test!("multi-word-event", "onMultiWordEvent");
    }

    // @evt="$in"
    macro_rules! test_with {
        ($ctx: ident, $in: literal, $expected: literal) => {
            let v_on = VOnDirective {
                event: Some("evt".into()),
                handler: Some(ts($in)),
                modifiers: vec![],
                span: DUMMY_SP,
            };
            let result = transform_v_on_base(
                &mut $ctx,
                &TransformNodeState::default(),
                &v_on,
                &element_from_tag("div"),
            )
            .expect("v-on with an event argument should produce a result")
            .result;
            let [property] = result.props.as_slice() else {
                panic!("v-on should produce exactly one property")
            };
            // Inspect handler before finish_v_on adds cache IR
            assert_eq!(
                $expected,
                property_to_str(property, AssertType::ExpressionNode),
                "handler for {}",
                $in
            );
        };
    }

    #[test]
    fn it_transforms_handler() {
        // `const foo = ref()`
        // `function func() {}`
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("foo"), BindingTypes::SetupRef),
            SetupBinding::new(fervid_atom!("func"), BindingTypes::SetupConst),
        ]);
        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(ctx, $in, $expected)
            };
        }

        // Explicit and known ref

        // Arrow
        test!("() => foo = 2", "()=>foo.value=2");
        test!("$event => foo = 2", "$event=>foo.value=2");
        test!("$event => foo = $event", "$event=>foo.value=$event");
        test!("v => foo = v", "v=>foo.value=v");
        test!("(v1, v2) => foo += v1 * v2", "(v1,v2)=>foo.value+=v1*v2");

        // Function
        test!("function () { foo = 2 }", "function(){foo.value=2;}");
        test!(
            "function assignFoo () { foo = 2 }",
            "function assignFoo(){foo.value=2;}"
        );
        test!(
            "function ($event) { foo = 2 }",
            "function($event){foo.value=2;}"
        );
        test!(
            "function ($event) { foo = $event }",
            "function($event){foo.value=$event;}"
        );
        test!("function (v) { foo = v }", "function(v){foo.value=v;}");
        test!(
            "function (v1, v2) { foo += v1 * v2 }",
            "function(v1,v2){foo.value+=v1*v2;}"
        );

        // Implicit $event
        test!("foo = 2", "$event=>foo.value=2");
        test!("foo = 2", "$event=>foo.value=2");
        test!("foo = $event", "$event=>foo.value=$event");

        // Different handler expressions:

        // resolved binding
        test!("func", "func");

        // unresolved binding
        test!("bar", "(...args)=>_ctx.bar&&_ctx.bar(...args)");

        // member expr
        test!(
            "foo.bar",
            "(...args)=>foo.value.bar&&foo.value.bar(...args)"
        );
        test!("bar.baz", "(...args)=>_ctx.bar.baz&&_ctx.bar.baz(...args)");

        // paren expr
        test!("(foo)", "(...args)=>(foo.value)&&(foo.value)(...args)");
        test!("(bar)", "(...args)=>(_ctx.bar)&&(_ctx.bar)(...args)");

        // ts non-null
        test!("foo!", "(...args)=>foo.value!&&foo.value!(...args)");
        test!("bar!", "(...args)=>_ctx.bar!&&_ctx.bar!(...args)");

        // optional chaining
        test!(
            "foo?.bar",
            "(...args)=>foo.value?.bar&&foo.value?.bar(...args)"
        );
        test!(
            "bar?.baz",
            "(...args)=>_ctx.bar?.baz&&_ctx.bar?.baz(...args)"
        );

        // call
        test!("func()", "$event=>func()");
        test!("func($event)", "$event=>func($event)");
        test!("foo($event)", "$event=>foo.value($event)");

        // array
        test!("[foo, bar]", "$event=>[foo.value,_ctx.bar]");

        // this
        test!("this", "$event=>this");

        // object
        // FIXME this is a bug in SWC stringifier (it should add parens):
        // or maybe it's not a bug, depends on if you interpret it as a Stmt or as an Expr
        // test!("{}", "$event=>({})");
        test!("{}", "$event=>{}");

        // unary
        test!("!foo", "$event=>!foo.value");

        // binary
        test!("foo || bar", "$event=>foo.value||_ctx.bar");

        // update
        test!("foo++", "$event=>foo.value++");

        // assign
        test!("foo += bar", "$event=>foo.value+=_ctx.bar");

        // new
        test!("new func", "$event=>new func");
        test!("new foo", "$event=>new foo.value");
        test!("new bar", "$event=>new _ctx.bar");

        // seq
        test!("foo, bar", "$event=>foo.value,_ctx.bar");

        // condition
        test!("foo ? bar : baz", "$event=>foo.value?_ctx.bar:_ctx.baz");

        // literal
        test!("123.45", "$event=>123.45");
        test!("'foo'", "$event=>\"foo\"");
        test!("true", "$event=>true");

        // template
        test!("`bar ${baz}`", "$event=>`bar ${_ctx.baz}`");

        // tagged template
        test!("foo`bar ${baz}`", "$event=>foo.value`bar ${_ctx.baz}`");

        // class
        test!("class FooBar {}", "$event=>class FooBar{}");
    }

    // https://github.com/vuejs/core/blob/fef2acb2049fce3407dff17fe8af1836b97dfd73/packages/compiler-sfc/__tests__/compileScript.spec.ts#L495-L543
    #[test]
    fn template_assignment_expression_codegen() {
        // ```ts
        // const count = ref(0)
        // const maybe = foo()
        // let lett = 1
        // let v = ref(1)
        // ```
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("count"), BindingTypes::SetupRef),
            SetupBinding::new(fervid_atom!("maybe"), BindingTypes::SetupMaybeRef),
            SetupBinding::new(fervid_atom!("lett"), BindingTypes::SetupLet),
            SetupBinding::new(fervid_atom!("v"), BindingTypes::SetupLet),
        ]);
        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(ctx, $in, $expected)
            };
        }

        // <div @click="count = 1"/>
        test!("count = 1", "$event=>count.value=1");

        // <div @click="maybe = count"/>
        test!(
            "maybe = count",
            // This is the official spec, but it is inconsistent with the `v-model` transform
            "$event=>maybe.value=count.value"
        );

        // <div @click="lett = count"/>
        test!(
            "lett = count",
            "$event=>_isRef(lett)?lett.value=count.value:lett=count.value"
        );

        // <div @click="v += 1"/>
        test!("v += 1", "$event=>_isRef(v)?v.value+=1:v+=1");

        // <div @click="v -= 1"/>
        test!("v -= 1", "$event=>_isRef(v)?v.value-=1:v-=1");

        // <div @click="() => {
        //     let a = '' + lett
        //     v = a
        // }"/>
        test!(
            "() => {
                let a = '' + lett
                v = a
            }",
            r#"()=>{let a=""+_unref(lett);_isRef(v)?v.value=a:v=a;}"#
        );

        // <div @click="() => {
        //     // nested scopes
        //     (()=>{
        //     let x = a
        //     (()=>{
        //         let z = x
        //         let z2 = z
        //     })
        //     let lz = z
        //     })
        //     v = a
        // }"/>
        test!(
            "() => {
                // nested scopes
                (()=>{
                    let x = a
                    (()=>{
                        let z = x
                        let z2 = z
                    })
                    let lz = z
                })
                v = a
            }",
            "()=>{(()=>{let x=_ctx.a(()=>{let z=x;let z2=z;});let lz=_ctx.z;});_isRef(v)?v.value=_ctx.a:v=_ctx.a;}"
        );
    }

    // https://github.com/vuejs/core/blob/fef2acb2049fce3407dff17fe8af1836b97dfd73/packages/compiler-sfc/__tests__/compileScript.spec.ts#L545-L574
    #[test]
    fn template_update_expression_codegen() {
        // ```ts
        // const count = ref(0)
        // const maybe = foo()
        // let lett = 1
        // ```
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("count"), BindingTypes::SetupRef),
            SetupBinding::new(fervid_atom!("maybe"), BindingTypes::SetupMaybeRef),
            SetupBinding::new(fervid_atom!("lett"), BindingTypes::SetupLet),
        ]);
        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(ctx, $in, $expected)
            };
        }

        // <div @click="count++"/>
        test!("count++", "$event=>count.value++");

        // <div @click="--count"/>
        test!("--count", "$event=>--count.value");

        // <div @click="maybe++"/>
        test!("maybe++", "$event=>maybe.value++");

        // <div @click="--maybe"/>
        test!("--maybe", "$event=>--maybe.value");

        // <div @click="lett++"/>
        test!("lett++", "$event=>_isRef(lett)?lett.value++:lett++");

        // <div @click="--lett"/>
        test!("--lett", "$event=>_isRef(lett)?--lett.value:--lett");
    }

    // https://github.com/vuejs/core/blob/fef2acb2049fce3407dff17fe8af1836b97dfd73/packages/compiler-sfc/__tests__/compileScript.spec.ts#L576-L600
    #[test]
    fn template_destructure_assignment_codegen() {
        // ```ts
        // const val = {}
        // const count = ref(0)
        // const maybe = foo()
        // let lett = 1
        // ```
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("val"), BindingTypes::SetupConst),
            SetupBinding::new(fervid_atom!("count"), BindingTypes::SetupRef),
            SetupBinding::new(fervid_atom!("maybe"), BindingTypes::SetupMaybeRef),
            SetupBinding::new(fervid_atom!("lett"), BindingTypes::SetupLet),
            SetupBinding::new(fervid_atom!("item"), BindingTypes::TemplateLocal),
        ]);
        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(ctx, $in, $expected)
            };
        }

        // Not a destructure, but an instant indicator if something is off
        test!("count = val", "$event=>count.value=val");

        // Template-local case
        // <div v-for="item in list"><div @click="({ item } = val)"/></div>
        test!("({ item } = val)", "$event=>({item}=val)");

        // <div @click="({ count } = val)"/>
        test!("({ count } = val)", "$event=>({count:count.value}=val)");

        // <div @click="[maybe] = val"/>
        test!("[maybe] = val", "$event=>[maybe.value]=val");

        // <div @click="({ lett } = val)"/>
        test!("({ lett } = val)", "$event=>({lett:lett}=val)");
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

    fn transform(
        event: StrOrExpr,
        handler: Option<&str>,
        modifiers: &[&str],
        node: &fervid_core::ElementNode,
    ) -> (TransformSfcContext, DirectiveTransformResult) {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.cache_handlers = false;
        let state = TransformNodeState::default();

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
        let result = transform_v_on(&mut ctx, &state, &directive, node)
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

    fn static_event_name(event: &str) -> String {
        let (_, result) = transform(event.into(), Some("handler"), &[], &element_from_tag("div"));
        static_key(only_property(&result)).to_owned()
    }

    fn with_bindings(bindings: Vec<SetupBinding>) -> TransformSfcContext {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.bindings_helper.setup_bindings.extend(bindings);
        ctx.bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
        ctx
    }
}
