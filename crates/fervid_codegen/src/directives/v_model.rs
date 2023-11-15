use fervid_core::{FervidAtom, StrOrExpr, VModelDirective};
use swc_core::{
    common::Span,
    ecma::ast::{
        BinExpr, BinaryOp, Bool, ComputedPropName, Expr, KeyValueProp, Lit, ObjectLit, Prop,
        PropName, PropOrSpread, Str,
    },
};

use crate::{
    context::CodegenContext,
    utils::{str_or_expr_to_propname, str_to_propname, to_camelcase},
};

impl CodegenContext {
    /// Returns true when v-model value was transformed
    pub fn generate_v_model_for_component(
        &self,
        v_model: &VModelDirective,
        out: &mut Vec<PropOrSpread>,
    ) -> bool {
        let span = v_model.span;
        let mut buf = String::new();

        // `v-model="smth"` is same as `v-model:modelValue="smth"`
        let bound_attribute = v_model
            .argument
            .to_owned()
            .unwrap_or_else(|| "modelValue".into());

        // 1. Transform the binding
        // let (transformed, has_js_bindings) =
        //     self.transform_v_model_value(v_model.value, scope_to_use, span);
        let has_js_bindings = true; // TODO

        // 2. Push model attribute and its binding,
        // e.g. `v-model="smth"` -> `modelValue: _ctx.smth`
        out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_or_expr_to_propname(bound_attribute.to_owned(), span),
            value: v_model.value.to_owned(),
        }))));

        // 3. Generate event name, e.g. `onUpdate:modelValue` or `onUpdate:usersArgument`
        // For dynamic model args, `["onUpdate:" + <model arg>]` will be generated
        let event_listener_propname = match bound_attribute {
            StrOrExpr::Str(ref s) => {
                buf.reserve(9 + s.len());
                buf.push_str("onUpdate:");
                let _ = to_camelcase(&s, &mut buf); // ignore fault
                str_to_propname(&buf, span)
            }

            StrOrExpr::Expr(ref expr) => {
                let addition = Expr::Bin(BinExpr {
                    span,
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Lit(Lit::Str(Str {
                        span,
                        value: FervidAtom::from("onUpdate:"),
                        raw: None,
                    }))),
                    right: expr.to_owned(),
                });

                PropName::Computed(ComputedPropName {
                    span,
                    expr: Box::new(addition),
                })
            }
        };

        // 4. Push the update code,
        // e.g. `v-model="smth"` -> `"onUpdate:modelValue": $event => ((_ctx.smth) = $event)`
        // TODO Cache like so `_cache[1] || (_cache[1] = `
        if let Some(ref update_handler) = v_model.update_handler {
            out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: event_listener_propname,
                value: update_handler.to_owned(),
            }))));
        }

        // 5. Optionally generate modifiers
        if v_model.modifiers.len() == 0 {
            return has_js_bindings;
        }

        // For regular model values, `<model name>Modifiers` is generated.
        // For dynamic ones - `[<model name> + "Modifiers"]`.
        let modifiers_propname = match bound_attribute {
            StrOrExpr::Str(model_arg) => {
                // Because we already used buffer for `event_listener`,
                // we can safely reuse it without allocating a new buffer
                buf.clear();

                // This is weird, but that's how the official compiler is implemented
                // modelValue => modelModifiers
                // users-argument => "users-argumentModifiers"
                if model_arg.eq("modelValue") {
                    buf.push_str("modelModifiers");
                } else {
                    buf.push_str(&model_arg);
                    buf.push_str("Modifiers");
                }

                str_to_propname(&buf, span)
            }

            StrOrExpr::Expr(expr) => {
                let addition = Expr::Bin(BinExpr {
                    span,
                    op: BinaryOp::Add,
                    left: expr.to_owned(),
                    right: Box::new(Expr::Lit(Lit::Str(Str {
                        span,
                        value: FervidAtom::from("Modifiers"),
                        raw: None,
                    }))),
                });

                PropName::Computed(ComputedPropName {
                    span,
                    expr: Box::new(addition),
                })
            }
        };

        out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: modifiers_propname,
            value: Box::new(Expr::Object(generate_v_model_modifiers(
                &v_model.modifiers,
                span,
            ))),
        }))));

        has_js_bindings
    }
}

fn generate_v_model_modifiers(modifiers: &[FervidAtom], span: Span) -> ObjectLit {
    let props = modifiers
        .iter()
        .map(|modifier| {
            // `.lazy` -> `lazy: true`
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: str_to_propname(&modifier, span),
                value: Box::new(Expr::Lit(Lit::Bool(Bool { span, value: true }))),
            })))
        })
        .collect();

    ObjectLit { span, props }
}

#[cfg(test)]
mod tests {
    use swc_core::{common::DUMMY_SP, ecma::ast::ObjectLit};

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_basic_usage() {
        // v-model="foo"
        test_out(
            vec![VModelDirective {
                argument: None,
                value: js("foo"),
                update_handler: js("$event=>((foo)=$event)").into(),
                modifiers: Vec::new(),
                span: DUMMY_SP,
            }],
            r#"{modelValue:foo,"onUpdate:modelValue":$event=>((foo)=$event)}"#,
        );
    }

    #[test]
    fn it_generates_named_model() {
        // v-model:simple="foo"
        test_out(
            vec![VModelDirective {
                argument: Some("simple".into()),
                value: js("foo"),
                update_handler: js("$event=>((foo)=$event)").into(),
                modifiers: Vec::new(),
                span: DUMMY_SP,
            }],
            r#"{simple:foo,"onUpdate:simple":$event=>((foo)=$event)}"#,
        );

        // v-model:modelValue="bar"
        test_out(
            vec![VModelDirective {
                argument: Some("modelValue".into()),
                value: js("bar"),
                update_handler: js("$event=>((bar)=$event)").into(),
                modifiers: Vec::new(),
                span: DUMMY_SP,
            }],
            r#"{modelValue:bar,"onUpdate:modelValue":$event=>((bar)=$event)}"#,
        );

        // v-model:model-value="baz"
        test_out(
            vec![VModelDirective {
                argument: Some("model-value".into()),
                value: js("baz"),
                update_handler: js("$event=>((baz)=$event)").into(),
                modifiers: Vec::new(),
                span: DUMMY_SP,
            }],
            r#"{"model-value":baz,"onUpdate:modelValue":$event=>((baz)=$event)}"#,
        );
    }

    #[test]
    fn it_generates_modifiers() {
        // v-model.lazy.trim="foo"
        test_out(
            vec![VModelDirective {
                argument: None,
                value: js("foo"),
                update_handler: js("$event=>((foo)=$event)").into(),
                modifiers: vec!["lazy".into(), "trim".into()],
                span: DUMMY_SP,
            }],
            r#"{modelValue:foo,"onUpdate:modelValue":$event=>((foo)=$event),modelModifiers:{lazy:true,trim:true}}"#,
        );

        // v-model.custom-modifier="foo"
        test_out(
            vec![VModelDirective {
                argument: None,
                value: js("foo"),
                update_handler: js("$event=>((foo)=$event)").into(),
                modifiers: vec!["custom-modifier".into()],
                span: DUMMY_SP,
            }],
            r#"{modelValue:foo,"onUpdate:modelValue":$event=>((foo)=$event),modelModifiers:{"custom-modifier":true}}"#,
        );

        // v-model:foo-bar.custom-modifier="bazQux"
        test_out(
            vec![VModelDirective {
                argument: Some("foo-bar".into()),
                value: js("bazQux"),
                update_handler: js("$event=>((bazQux)=$event)").into(),
                modifiers: vec!["custom-modifier".into()],
                span: DUMMY_SP,
            }],
            r#"{"foo-bar":bazQux,"onUpdate:fooBar":$event=>((bazQux)=$event),"foo-barModifiers":{"custom-modifier":true}}"#,
        );
    }

    #[test]
    fn it_generates_dynamic_model_name() {
        // v-model:[foo]="bar"
        test_out(
            vec![VModelDirective {
                argument: Some(StrOrExpr::Expr(js("foo"))),
                value: js("bar"),
                update_handler: js("$event=>((bar)=$event)").into(),
                modifiers: Vec::new(),
                span: DUMMY_SP,
            }],
            r#"{[foo]:bar,["onUpdate:"+foo]:$event=>((bar)=$event)}"#,
        );

        // v-model:[foo].baz="bar"
        test_out(
            vec![VModelDirective {
                argument: Some(StrOrExpr::Expr(js("foo"))),
                value: js("bar"),
                update_handler: js("$event=>((bar)=$event)").into(),
                modifiers: vec!["baz".into()],
                span: DUMMY_SP,
            }],
            r#"{[foo]:bar,["onUpdate:"+foo]:$event=>((bar)=$event),[foo+"Modifiers"]:{baz:true}}"#,
        );
    }

    fn test_out(input: Vec<VModelDirective>, expected: &str) {
        let ctx = CodegenContext::default();
        let mut out = ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        };
        for v_model in input.iter() {
            ctx.generate_v_model_for_component(v_model, &mut out.props);
        }
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
