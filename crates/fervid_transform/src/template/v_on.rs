use fervid_core::{
    fervid_atom, BindingTypes, FervidAtom, IntoIdent, StrOrExpr, VOnDirective, VueImports,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrowExpr, BinExpr, BinaryOp, BindingIdent, BlockStmtOrExpr, CallExpr, Callee, Expr,
        ExprOrSpread, Ident, Pat, RestPat,
    },
};

use super::{
    ast_transform::TemplateVisitor,
    expr_transform::BindingsHelperTransform,
    utils::{to_camel_case, to_pascal_case, wrap_in_event_arrow},
};

impl TemplateVisitor<'_> {
    pub fn transform_v_on(&mut self, v_on: &mut VOnDirective, scope_to_use: u32) {
        match v_on.event.as_mut() {
            Some(StrOrExpr::Str(static_event)) => {
                transform_v_on_static_event(static_event);
            }

            Some(StrOrExpr::Expr(dynamic_event)) => {
                self.ctx
                    .bindings_helper
                    .transform_expr(dynamic_event, scope_to_use);

                // _toHandlerKey
                let to_handler_key_ident = VueImports::ToHandlerKey.as_atom();
                self.ctx.bindings_helper.vue_imports |= VueImports::ToHandlerKey;

                // Wrap in `_toHandlerKey`
                let value = std::mem::take(dynamic_event);
                *dynamic_event = Box::new(Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                        sym: to_handler_key_ident,
                        ..Default::default()
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: value,
                    }],
                    type_args: None,
                }));
            }

            None => {}
        }

        if let Some(mut handler) = v_on.handler.take() {
            // 1. Check the handler shape
            let mut is_member_or_paren = false;
            let mut is_non_null_or_opt_chain = false;
            let mut is_non_const_ident = false;
            let mut needs_event = false;

            match unwrap_parens(&handler) {
                // This is always as-is
                Expr::Fn(_) | Expr::Arrow(_) => {}

                // This is either as-is (if const) or `(...args) => _ctx.smth && _ctx.smth(...args)`
                Expr::Ident(ident) => {
                    is_non_const_ident = !matches!(
                        self.ctx
                            .bindings_helper
                            .get_var_binding_type(scope_to_use, &ident.sym),
                        BindingTypes::SetupConst
                            | BindingTypes::LiteralConst
                            | BindingTypes::SetupReactiveConst
                    );
                }

                // This is getting `(...args) => _ctx.smth && _ctx.smth(...args)`
                Expr::Member(_) | Expr::Paren(_) => {
                    is_member_or_paren = true;
                }

                // According to the user, we do not need `_ctx.smth &&` check
                Expr::TsNonNull(_) | Expr::OptChain(_) => {
                    is_non_null_or_opt_chain = true;
                }

                // This is getting `$event =>`
                Expr::Call(_)
                | Expr::Array(_)
                | Expr::This(_)
                | Expr::Object(_)
                | Expr::Unary(_)
                | Expr::Bin(_)
                | Expr::Update(_)
                | Expr::Assign(_)
                | Expr::New(_)
                | Expr::Seq(_)
                | Expr::Cond(_)
                | Expr::Lit(_)
                | Expr::Tpl(_)
                | Expr::Class(_)
                | Expr::TaggedTpl(_) => {
                    needs_event = true;
                }

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
                _ => {}
            }

            // 2. Add `$event` when needed
            if needs_event {
                handler = wrap_in_event_arrow(handler);
            }

            // 3. Transform the handler
            self.ctx
                .bindings_helper
                .transform_expr(&mut handler, scope_to_use);

            // 4. Wrap in `(...args)` arrow if needed
            if is_non_const_ident || is_member_or_paren || is_non_null_or_opt_chain {
                handler = wrap_in_args_arrow(handler, !is_non_null_or_opt_chain);
            }

            // Re-assign because it was `take`n
            v_on.handler = Some(handler);
        }
    }
}

#[inline]
fn transform_v_on_static_event(static_event: &mut FervidAtom) {
    let transformed_event = if static_event.starts_with("vue:") {
        // `vue:` events have special transform
        // `vue:update` -> `onVnodeUpdate`
        let replaced = static_event.replacen("vue:", "vnode-", 1);
        let mut out = String::with_capacity(replaced.len() + 2);
        out.push_str("on");
        to_pascal_case(&replaced, &mut out);
        out
    } else {
        camelcase_with_word_groups(static_event)
    };

    *static_event = FervidAtom::from(transformed_event);
}

/// Turns an event name to an `on` event handler.
/// The algorithm is as follows:
/// 0. Push `on` to buffer;
/// 1. Check that event is all not-uppercase.
///    a. If it is, push `:` and event itself to the buffer. Return;
///    b. Otherwise set `is_capital=true`;
/// 2. Read ASCII letters and `-` symbols until non-matching character or EOL;
/// 3. Convert the read sequence into camel-case if `is_capital=false`; or pascal-case otherwise;
/// 4. Push converted sequence and non-matching character;
/// 5. Repeat steps 2-5 until EOL.
fn camelcase_with_word_groups(mut from: &str) -> String {
    let mut buf = String::with_capacity(from.len() + 3);
    buf.push_str("on");

    if from.chars().any(|c| c.is_ascii_uppercase()) {
        buf.push(':');
        buf.push_str(from);
        return buf;
    }

    let mut is_capital = true;

    while !from.is_empty() {
        let non_matching_idx = from
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .unwrap_or(from.len());

        let matching = &from[..non_matching_idx];

        if is_capital {
            to_pascal_case(matching, &mut buf);
            is_capital = false;
        } else {
            to_camel_case(matching, &mut buf);
        }

        from = &from[non_matching_idx..];

        if let Some(c) = from.chars().next() {
            buf.push(c);
            from = &from[c.len_utf8()..];
        }
    }

    buf
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
        body: Box::new(BlockStmtOrExpr::Expr(expr)),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    }))
}

// Mirror what `@babel/parser` does
fn unwrap_parens(expr: &Expr) -> &Expr {
    let Expr::Paren(p) = expr else {
        return expr;
    };

    &p.expr
}

#[cfg(test)]
mod tests {
    use fervid_core::{fervid_atom, BindingTypes, TemplateGenerationMode};

    use crate::{
        test_utils::{to_str, ts},
        SetupBinding, TransformSfcContext,
    };

    use super::*;

    #[test]
    fn camelcase_with_word_groups_works() {
        // Only lowercase gets sent to this function
        assert_eq!(
            camelcase_with_word_groups("update:model-value"),
            "onUpdate:modelValue"
        );
        assert_eq!(
            camelcase_with_word_groups("update!model-value"),
            "onUpdate!modelValue"
        );
        assert_eq!(
            camelcase_with_word_groups("update!!model-value"),
            "onUpdate!!modelValue"
        );
        assert_eq!(
            camelcase_with_word_groups("update-model-value"),
            "onUpdateModelValue"
        );
    }

    #[test]
    fn it_transforms_static_event() {
        macro_rules! test {
            ($from: literal, $to: literal) => {{
                let mut atom = fervid_atom!($from);
                transform_v_on_static_event(&mut atom);
                assert_eq!(&atom, $to);
            }};
        }

        test!("update:model-value", "onUpdate:modelValue");
        test!("update:modelValue", "on:update:modelValue");
        test!("vue:update", "onVnodeUpdate");
        test!("vue:updateFoo", "onVnodeUpdateFoo");
        test!("click", "onClick");
        test!("multi-word-event", "onMultiWordEvent");
    }

    // @evt="$in"
    macro_rules! test_with {
        ($visitor: ident, $in: literal, $expected: literal) => {
            let mut v_on = VOnDirective {
                event: Some("evt".into()),
                handler: Some(ts($in)),
                modifiers: vec![],
                span: DUMMY_SP,
            };
            $visitor.transform_v_on(&mut v_on, 0);
            assert_eq!($expected, to_str(&v_on.handler.expect("should exist")));
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
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(template_visitor, $in, $expected)
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
        test!("foo!", "(...args)=>foo.value!(...args)");
        test!("bar!", "(...args)=>_ctx.bar!(...args)");

        // optional chaining
        test!("foo?.bar", "(...args)=>foo.value?.bar(...args)");
        test!("bar?.baz", "(...args)=>_ctx.bar?.baz(...args)");

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
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(template_visitor, $in, $expected)
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
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(template_visitor, $in, $expected)
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
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! test {
            ($in: literal, $expected: literal) => {
                test_with!(template_visitor, $in, $expected)
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

    fn with_bindings(bindings: Vec<SetupBinding>) -> TransformSfcContext {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.bindings_helper.setup_bindings.extend(bindings);
        ctx.bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
        ctx
    }
}
