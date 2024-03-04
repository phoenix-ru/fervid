use fervid_core::{fervid_atom, FervidAtom, StrOrExpr, VOnDirective};
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
                self.bindings_helper
                    .transform_expr(dynamic_event, scope_to_use);
            }

            None => {}
        }

        if let Some(mut handler) = v_on.handler.take() {
            // 1. Check the handler shape
            let mut is_member_or_paren = false;
            let mut is_non_null_or_opt_chain = false;
            let mut is_unresolved_ident = false;
            let mut needs_event = false;

            match handler.as_ref() {
                // This is always as-is
                Expr::Fn(_) | Expr::Arrow(_) => {}

                // This is either as-is (if known) or `(...args) => _ctx.smth && _ctx.smth(...args)`
                Expr::Ident(ident) => {
                    is_unresolved_ident = matches!(
                        self.bindings_helper
                            .get_var_binding_type(scope_to_use, &ident.sym),
                        fervid_core::BindingTypes::Unresolved
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
            self.bindings_helper
                .transform_expr(&mut handler, scope_to_use);

            // 4. Wrap in `(...args)` arrow if needed
            if is_unresolved_ident || is_member_or_paren || is_non_null_or_opt_chain {
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
        camelcase_with_word_groups(&static_event)
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

    let args_ident = Ident {
        span: DUMMY_SP,
        sym: fervid_atom!("args"),
        optional: false,
    };

    // Modify to call expression
    expr = Box::new(Expr::Call(CallExpr {
        span: DUMMY_SP,
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
        params: vec![args_pat],
        body: Box::new(BlockStmtOrExpr::Expr(expr)),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    }))
}

#[cfg(test)]
mod tests {
    use fervid_core::fervid_atom;

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
    }
}
