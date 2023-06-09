use fervid_core::VModelDirective;
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::{
        ast::{
            ArrowExpr, AssignExpr, AssignOp, BindingIdent, BlockStmtOrExpr, Bool, Expr, Ident,
            Invalid, KeyValueProp, Lit, ObjectLit, ParenExpr, Pat, PatOrExpr, Prop, PropOrSpread,
        },
        atoms::JsWord,
    },
};

use crate::{
    context::CodegenContext,
    transform::{transform_scoped, MockScopeHelper},
    utils::{is_valid_ident, str_to_propname, to_camelcase},
};

impl CodegenContext {
    pub fn generate_v_model_for_component(
        &self,
        v_model: &VModelDirective,
        out: &mut Vec<PropOrSpread>,
        scope_to_use: u32,
    ) {
        // TODO Spans
        let span = DUMMY_SP;

        // Empty v-models are skipped.
        // They should be reported in the analyzer
        if v_model.value == "" {
            return;
        }

        // `v-model="smth"` is same as `v-model:modelValue="smth"`
        let bound_attribute = v_model.argument.unwrap_or("modelValue");

        // 1. Transform the binding
        let transformed = self.transform_v_model_value(v_model.value, scope_to_use, span);

        // 2. Push model attribute and its binding,
        // e.g. `v-model="smth"` -> `modelValue: _ctx.smth`
        out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_to_propname(bound_attribute, span),
            value: transformed,
        }))));

        // 3. Generate event name, e.g. `onUpdate:modelValue` or `onUpdate:usersArgument`
        let mut event_listener = String::with_capacity(9 + bound_attribute.len());
        event_listener.push_str("onUpdate:");
        let _ = to_camelcase(bound_attribute, &mut event_listener); // ignore fault

        // 4. Push the update code,
        // e.g. `v-model="smth"` -> `"onUpdate:modelValue": $event => ((_ctx.smth) = $event)`
        out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_to_propname(&event_listener, span),
            value: self.generate_v_model_update_fn(v_model.value, scope_to_use, span),
        }))));

        // 5. Optionally generate modifiers
        if v_model.modifiers.len() == 0 {
            return;
        }

        // Because we already used `event_listener` buffer,
        // we can safely reuse it without allocating a new buffer
        let mut modifiers_prop = event_listener;
        modifiers_prop.clear();

        // This is weird, but that's how the official compiler is implemented
        // modelValue => modelModifiers
        // users-argument => "users-argumentModifiers"
        if bound_attribute == "modelValue" {
            modifiers_prop.push_str("modelModifiers");
        } else {
            modifiers_prop.push_str(bound_attribute);
            modifiers_prop.push_str("Modifiers");
        }

        out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_to_propname(&modifiers_prop, span),
            value: Box::new(Expr::Object(generate_v_model_modifiers(
                &v_model.modifiers,
                span,
            ))),
        }))));
    }

    /// Transforms the binding of `v-model`.
    /// Because the rules of transformation differ a lot depending on the `BindingType`,
    /// transformed expression may also differ a lot.
    fn transform_v_model_value(&self, value: &str, scope_to_use: u32, span: Span) -> Box<Expr> {
        // TODO Implement the correct transformation based on BindingTypes
        transform_scoped(value, &MockScopeHelper, scope_to_use)
            .unwrap_or_else(|| Box::new(Expr::Invalid(Invalid { span })))
    }

    /// Generates the update code for the `v-model`.
    /// Same as [`transform_v_model_value`], logic may differ a lot.
    fn generate_v_model_update_fn(&self, value: &str, scope_to_use: u32, span: Span) -> Box<Expr> {
        // TODO Actual implementation

        // todo maybe re-use the previously generated expression from generate_v_model_for_component?
        let transformed_v_model = self.transform_v_model_value(value, scope_to_use, span);

        // $event => ((_ctx.modelValue) = $event)
        Box::new(Expr::Arrow(ArrowExpr {
            span,
            params: vec![Pat::Ident(BindingIdent {
                id: Ident {
                    span,
                    sym: JsWord::from("$event"),
                    optional: false,
                },
                type_ann: None,
            })],
            body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Paren(ParenExpr {
                span,
                expr: Box::new(Expr::Assign(AssignExpr {
                    span,
                    op: AssignOp::Assign,
                    left: PatOrExpr::Expr(Box::new(Expr::Paren(ParenExpr {
                        span,
                        expr: transformed_v_model,
                    }))),
                    right: Box::new(Expr::Ident(Ident {
                        span,
                        sym: JsWord::from("$event"),
                        optional: false,
                    })),
                })),
            })))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        }))
    }
}

fn generate_v_model_modifiers(modifiers: &[&str], span: Span) -> ObjectLit {
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
    use std::sync::Arc;

    use swc_core::{common::SourceMap, ecma::ast::ObjectLit};
    use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};

    use super::*;

    #[test]
    fn it_generates_basic_usage() {
        // v-model="foo"
        test_out(
            vec![VModelDirective {
                argument: None,
                value: "foo",
                modifiers: Vec::new(),
            }],
            r#"{modelValue:_ctx.foo,"onUpdate:modelValue":$event=>((_ctx.foo)=$event)}"#,
        );
    }

    #[test]
    fn it_generates_named_model() {
        // v-model:simple="foo"
        test_out(
            vec![VModelDirective {
                argument: Some("simple"),
                value: "foo",
                modifiers: Vec::new(),
            }],
            r#"{simple:_ctx.foo,"onUpdate:simple":$event=>((_ctx.foo)=$event)}"#,
        );

        // v-model:modelValue="bar"
        test_out(
            vec![VModelDirective {
                argument: Some("modelValue"),
                value: "bar",
                modifiers: Vec::new(),
            }],
            r#"{modelValue:_ctx.bar,"onUpdate:modelValue":$event=>((_ctx.bar)=$event)}"#,
        );

        // v-model:model-value="baz"
        test_out(
            vec![VModelDirective {
                argument: Some("model-value"),
                value: "baz",
                modifiers: Vec::new(),
            }],
            r#"{"model-value":_ctx.baz,"onUpdate:modelValue":$event=>((_ctx.baz)=$event)}"#,
        );
    }

    #[test]
    fn it_generates_modifiers() {
        // v-model.lazy.trim="foo"
        test_out(
            vec![VModelDirective {
                argument: None,
                value: "foo",
                modifiers: vec!["lazy", "trim"],
            }],
            r#"{modelValue:_ctx.foo,"onUpdate:modelValue":$event=>((_ctx.foo)=$event),modelModifiers:{lazy:true,trim:true}}"#,
        );

        // v-model.custom-modifier="foo"
        test_out(
            vec![VModelDirective {
                argument: None,
                value: "foo",
                modifiers: vec!["custom-modifier"],
            }],
            r#"{modelValue:_ctx.foo,"onUpdate:modelValue":$event=>((_ctx.foo)=$event),modelModifiers:{"custom-modifier":true}}"#,
        );

        // v-model:foo-bar.custom-modifier="bazQux"
        test_out(
            vec![VModelDirective {
                argument: Some("foo-bar"),
                value: "bazQux",
                modifiers: vec!["custom-modifier"],
            }],
            r#"{"foo-bar":_ctx.bazQux,"onUpdate:fooBar":$event=>((_ctx.bazQux)=$event),"foo-barModifiers":{"custom-modifier":true}}"#,
        );
    }

    fn test_out(input: Vec<VModelDirective>, expected: &str) {
        let ctx = CodegenContext::default();
        let mut out = ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        };
        for v_model in input.iter() {
            ctx.generate_v_model_for_component(v_model, &mut out.props, 0);
        }
        assert_eq!(to_str(out), expected)
    }

    fn to_str(obj_lit: ObjectLit) -> String {
        // Emitting the result requires some setup with SWC
        let cm: Arc<SourceMap> = Default::default();
        let mut buff: Vec<u8> = Vec::with_capacity(128);
        let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config {
                target: Default::default(),
                ascii_only: false,
                minify: true,
                omit_last_semi: false,
            },
            comments: None,
            wr: writer,
            cm,
        };

        let _ = obj_lit.emit_with(&mut emitter);

        String::from_utf8(buff).unwrap()
    }
}
