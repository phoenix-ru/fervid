use fervid_core::{HtmlAttribute, VBindDirective, VDirective, VOnDirective};
use regex::Regex;
use smallvec::SmallVec;
use swc_core::{
    common::{Span, Spanned, DUMMY_SP},
    ecma::{
        ast::{
            ArrayLit, ArrowExpr, BinExpr, BinaryOp, BlockStmt, BlockStmtOrExpr, CallExpr, Callee,
            ComputedPropName, Expr, ExprOrSpread, Ident, Invalid, KeyValueProp, Lit, ObjectLit,
            Prop, PropName, PropOrSpread, Str,
        },
        atoms::{js_word, Atom, JsWord},
    },
};

use crate::{
    context::CodegenContext,
    imports::VueImports,
    transform::{transform_scoped, MockScopeHelper},
    utils::{str_to_propname, to_pascalcase},
};

lazy_static! {
    static ref CSS_RE: Regex =
        Regex::new(r"(?U)([a-zA-Z_-][a-zA-Z_0-9-]*):\s*(.+)(?:;|$)").unwrap();
}

/// Type alias for all the directives not handled as attributes.
/// Only `v-on` and `v-bind` as well as `v-model` for components generate attribute code.
/// Other directives have their own specifics of code generation, which are handled separately.
pub type DirectivesToProcess<'i> = SmallVec<[&'i VDirective<'i>; 2]>;

#[derive(Debug, Default)]
pub struct GenerateAttributesResultHints<'i> {
    // _normalizeProps({
    //     foo: "bar",
    //     [_ctx.dynamic || ""]: _ctx.hi
    // })
    pub needs_normalize_props: bool,

    /// When `v-bind="smth"` was found
    pub v_bind_no_arg: Option<&'i VBindDirective<'i>>,

    /// When `v-on="smth"` was found
    pub v_on_no_event: Option<&'i VOnDirective<'i>>,
}

impl CodegenContext {
    pub fn generate_attributes<'attr>(
        &mut self,
        attributes: &'attr [HtmlAttribute],
        out: &mut Vec<PropOrSpread>,
        unsupported_directives: &mut DirectivesToProcess<'attr>,
        template_scope_id: u32,
    ) -> GenerateAttributesResultHints<'attr> {
        // Special generation for `class` and `style` attributes,
        // as they can have both Regular and VDirective variants
        let mut class_regular_attr = None;
        let mut class_bound = None;
        let mut style_regular_attr = None;
        let mut style_bound = None;

        // Hints on what was processed and what to do next
        let mut result_hints = GenerateAttributesResultHints::default();

        for attribute in attributes {
            // TODO Spans
            let span = DUMMY_SP;

            match attribute {
                // First, we check the special case: `class` and `style` attributes
                // class
                HtmlAttribute::Regular {
                    name: "class",
                    value,
                } => {
                    class_regular_attr = Some((*value, span));
                }

                // style
                HtmlAttribute::Regular {
                    name: "style",
                    value,
                } => {
                    style_regular_attr = Some((*value, span));
                }

                // Any regular attribute will be added as an object entry,
                // where key is attribute name and value is attribute value as string literal
                HtmlAttribute::Regular { name, value } => {
                    out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                        KeyValueProp {
                            key: str_to_propname(&name, span),
                            value: Box::from(Expr::Lit(Lit::Str(Str {
                                span,
                                value: JsWord::from(*value),
                                raw: Some(Atom::from(*value)),
                            }))),
                        },
                    ))));
                }

                // Directive.
                // `v-on` and `v-bind` are processed here, other directives
                // will be added to the vector of unprocessed directives
                HtmlAttribute::VDirective(directive) => {
                    match directive {
                        // :class
                        VDirective::Bind(VBindDirective {
                            argument: Some("class"),
                            value,
                            ..
                        }) => {
                            class_bound = Some((*value, span));
                        }

                        // :style
                        VDirective::Bind(VBindDirective {
                            argument: Some("style"),
                            value,
                            ..
                        }) => {
                            style_bound = Some((*value, span));
                        }

                        // `v-bind` directive without argument needs its own processing
                        VDirective::Bind(v_bind) if v_bind.argument.is_none() => {
                            // IN:
                            // v-on="ons" v-bind="bounds" @click=""
                            //
                            // OUT:
                            // _mergeProps(_toHandlers(_ctx.ons), _ctx.bounds, {
                            //   onClick: _cache[1] || (_cache[1] = () => {})
                            // })
                            result_hints.v_bind_no_arg = Some(v_bind);
                        }

                        // `v-on` directive without event name also needs its own processing
                        VDirective::On(v_on) if v_on.event.is_none() => {
                            result_hints.v_on_no_event = Some(v_on);
                        }

                        // `v-bind` directive, shortcut `:`, e.g. `:custom-prop="value"`
                        VDirective::Bind(VBindDirective {
                            argument: Some(argument),
                            value,
                            is_dynamic_attr,
                            ..
                        }) => {
                            // Skip empty `v-bind`s. They should be reported in AST analyzer
                            if value.len() == 0 {
                                continue;
                            }

                            // Dynamic prop needs a `_normalizeProps` call
                            if *is_dynamic_attr {
                                result_hints.needs_normalize_props = true;
                            }

                            // Transform the raw expression
                            let transformed =
                                transform_scoped(&value, &MockScopeHelper, template_scope_id)
                                    .unwrap_or_else(|| Box::new(Expr::Invalid(Invalid { span })));

                            let key = if *is_dynamic_attr {
                                // For dynamic attributes, keys are in form `[_ctx.dynamic || ""]`
                                let key_transformed =
                                    transform_scoped(&value, &MockScopeHelper, template_scope_id)
                                        .unwrap_or_else(|| {
                                            Box::new(Expr::Invalid(Invalid { span }))
                                        });

                                // `[key_transformed || ""]`
                                PropName::Computed(ComputedPropName {
                                    span,
                                    expr: Box::from(Expr::Bin(BinExpr {
                                        span,
                                        op: BinaryOp::LogicalOr,
                                        left: Box::from(key_transformed),
                                        right: Box::from(Expr::Lit(Lit::Str(Str {
                                            span,
                                            value: JsWord::from(""),
                                            raw: None,
                                        }))),
                                    })),
                                })
                            } else {
                                str_to_propname(argument, span)
                            };

                            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                                KeyValueProp {
                                    key,
                                    value: transformed,
                                },
                            ))));
                        }

                        // v-on directive, shortcut `@`, e.g. `@custom-event.modifier="value"`
                        VDirective::On(VOnDirective {
                            event: Some(event),
                            handler,
                            is_dynamic_event,
                            modifiers,
                        }) => {
                            // TODO Use _cache

                            // Transform or default to () => {}
                            let transformed = handler
                                .and_then(|handler| {
                                    transform_scoped(handler, &MockScopeHelper, template_scope_id)
                                })
                                .unwrap_or_else(|| Box::new(empty_arrow_expr(span)));

                            // To generate as an array of `["modifier1", "modifier2"]`
                            let modifiers: Vec<Option<ExprOrSpread>> = modifiers
                                .iter()
                                .map(|modifier| {
                                    Some(ExprOrSpread {
                                        spread: None,
                                        expr: Box::from(Expr::Lit(Lit::Str(Str {
                                            span,
                                            value: JsWord::from(*modifier),
                                            raw: None,
                                        }))),
                                    })
                                })
                                .collect();

                            let handler_expr = if modifiers.len() != 0 {
                                let with_modifiers_import =
                                    self.get_and_add_import_ident(VueImports::WithModifiers);

                                // `_withModifiers(transformed, ["modifier"]))`
                                Box::new(Expr::Call(CallExpr {
                                    span,
                                    callee: Callee::Expr(Box::from(Expr::Ident(Ident {
                                        span,
                                        sym: with_modifiers_import,
                                        optional: false,
                                    }))),
                                    args: vec![
                                        ExprOrSpread {
                                            expr: Box::from(transformed),
                                            spread: None,
                                        },
                                        ExprOrSpread {
                                            expr: Box::from(Expr::Array(ArrayLit {
                                                span,
                                                elems: modifiers,
                                            })),
                                            spread: None,
                                        },
                                    ],
                                    type_args: None,
                                }))
                            } else {
                                // No modifiers, leave expression the same
                                transformed
                            };

                            // TODO Cache

                            // TODO Dynamic events are hard, but similar to `v-on`
                            // IN:
                            // foo="bar" :[dynamic]="hi" @[dynamic]="" @[dynamic2]="" v-on="whatever"
                            //
                            // OUT:
                            // _mergeProps({
                            //     foo: "bar",
                            //     [_ctx.dynamic || ""]: _ctx.hi
                            // }, {
                            //     [_toHandlerKey(_ctx.dynamic)]: _cache[4] || (_cache[4] = () => {})
                            // }, {
                            //     [_toHandlerKey(_ctx.dynamic2)]: _cache[5] || (_cache[5] = () => {})
                            // }, _toHandlers(whatever, true))

                            // IDEA: Do the generation here, and put resulting `Expr`s in the return struct

                            let event_name = event_name_to_handler(event);

                            // e.g. `onClick: _ctx.handleClick` or `onClick: _withModifiers(() => {}, ["stop"])
                            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                                KeyValueProp {
                                    key: str_to_propname(&event_name, span),
                                    value: handler_expr,
                                },
                            ))));
                        }

                        _ => {
                            unsupported_directives.push(directive)
                        }
                    }
                }
            }
        }

        self.generate_class_bindings(class_regular_attr, class_bound, out, template_scope_id);
        self.generate_style_bindings(style_regular_attr, style_bound, out, template_scope_id);

        result_hints
    }

    /// Process `class` attribute. We may have a regular one, a bound one, both or neither.
    fn generate_class_bindings(
        &mut self,
        class_regular_attr: Option<(&str, Span)>,
        class_bound: Option<(&str, Span)>,
        out: &mut Vec<PropOrSpread>,
        scope_to_use: u32,
    ) {
        let mut expr: Option<Expr> = None;

        match (class_regular_attr, class_bound) {
            // Both regular `class` and bound `:class`
            (Some((regular_value, regular_span)), Some((bound_value, bound_span))) => {
                // 1. []
                // Normalize class with both `class` and `:class` needs an array
                let mut normalize_array = ArrayLit {
                    span: bound_span, // Idk which span should be used here
                    elems: Vec::with_capacity(2),
                };

                // 2. ["regular classes"]
                // Include the content of a regular class
                normalize_array.elems.push(Some(ExprOrSpread {
                    spread: None,
                    expr: Box::from(Expr::Lit(Lit::Str(Str {
                        span: regular_span,
                        value: regular_value.into(),
                        raw: Some(regular_value.into()),
                    }))),
                }));

                // 3. Transform the bound value
                let transformed = transform_scoped(bound_value, &MockScopeHelper, scope_to_use)
                    .unwrap_or_else(|| Box::new(Expr::Invalid(Invalid { span: bound_span })));

                // 4. ["regular classes", boundClasses]
                normalize_array.elems.push(Some(ExprOrSpread {
                    spread: None,
                    expr: transformed,
                }));

                // `normalizeClass(["regular classes", boundClasses])`
                expr = Some(Expr::Call(CallExpr {
                    span: bound_span,
                    callee: Callee::Expr(Box::from(Expr::Ident(Ident {
                        span: bound_span,
                        sym: self.get_and_add_import_ident(VueImports::NormalizeClass),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::from(Expr::Array(normalize_array)),
                    }],
                    type_args: None,
                }));
            }

            // Just regular `class`
            (Some((regular_value, span)), None) => {
                expr = Some(Expr::Lit(Lit::Str(Str {
                    raw: Some(regular_value.into()),
                    value: regular_value.into(),
                    span,
                })));
            }

            // Just bound `:class`
            (None, Some((bound_value, span))) => {
                let transformed = transform_scoped(bound_value, &MockScopeHelper, scope_to_use)
                    .unwrap_or_else(|| Box::new(Expr::Invalid(Invalid { span })));

                // `normalizeClass(boundClasses)`
                expr = Some(Expr::Call(CallExpr {
                    span,
                    callee: Callee::Expr(Box::from(Expr::Ident(Ident {
                        span,
                        sym: self.get_and_add_import_ident(VueImports::NormalizeClass),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: transformed,
                    }],
                    type_args: None,
                }));
            }

            // Neither
            (None, None) => {}
        }

        // Add `class` to attributes
        if let Some(expr) = expr {
            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                KeyValueProp {
                    key: PropName::Ident(Ident::new(js_word!("class"), expr.span())),
                    value: Box::from(expr),
                },
            ))));
        }
    }

    fn generate_style_bindings(
        &mut self,
        style_regular_attr: Option<(&str, Span)>,
        style_bound: Option<(&str, Span)>,
        out: &mut Vec<PropOrSpread>,
        scope_to_use: u32
    ) {
        let mut expr = None;

        match (style_regular_attr, style_bound) {
            // Both `style` and `:style`
            (Some((regular_value, regular_span)), Some((bound_value, bound_span))) => {
                // 1. []
                // normalizeStyle with both `style` and `:style` needs an array
                let mut normalize_array = ArrayLit {
                    span: bound_span, // Idk which span should be used here
                    elems: Vec::with_capacity(2),
                };

                // 2. { regular: "styles as an object" }
                // Generate the regular styles into an object
                let regular_styles_obj = generate_regular_style(regular_value, regular_span);

                // 3. [{ regular: "styles as an object" }]
                // Include the content of a regular style
                normalize_array.elems.push(Some(ExprOrSpread {
                    spread: None,
                    expr: Box::from(Expr::Object(regular_styles_obj)),
                }));

                // 4. Transform the bound value
                let transformed = transform_scoped(bound_value, &MockScopeHelper, scope_to_use)
                    .unwrap_or_else(|| Box::new(Expr::Invalid(Invalid { span: bound_span })));

                // 5. [{ regular: "styles as an object" }, boundStyles]
                normalize_array.elems.push(Some(ExprOrSpread {
                    spread: None,
                    expr: transformed,
                }));

                // `normalizeClass([{ regular: "styles as an object" }, boundStyles])`
                expr = Some(Expr::Call(CallExpr {
                    span: bound_span,
                    callee: Callee::Expr(Box::from(Expr::Ident(Ident {
                        span: bound_span,
                        sym: self.get_and_add_import_ident(VueImports::NormalizeStyle),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::from(Expr::Array(normalize_array)),
                    }],
                    type_args: None,
                }));
            }

            // `style`
            (Some((regular_value, span)), None) => {
                expr = Some(Expr::Object(generate_regular_style(regular_value, span)));
            }

            // `:style`
            (None, Some((bound_value, span))) => {
                let transformed = transform_scoped(bound_value, &MockScopeHelper, scope_to_use)
                    .unwrap_or_else(|| Box::new(Expr::Invalid(Invalid { span })));

                // `normalizeStyle(boundStyles)`
                expr = Some(Expr::Call(CallExpr {
                    span,
                    callee: Callee::Expr(Box::from(Expr::Ident(Ident {
                        span,
                        sym: self.get_and_add_import_ident(VueImports::NormalizeStyle),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: transformed,
                    }],
                    type_args: None,
                }));
            }

            (None, None) => {}
        }

        // Add `style` to attributes
        if let Some(expr) = expr {
            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                KeyValueProp {
                    key: PropName::Ident(Ident::new(js_word!("style"), expr.span())),
                    value: Box::from(expr),
                },
            ))));
        }
    }
}

fn generate_regular_style(style: &str, span: Span) -> ObjectLit {
    let mut result = ObjectLit {
        span,
        props: Vec::with_capacity(4), // pre-allocate more just in case
    };

    for mat in CSS_RE.captures_iter(style) {
        let Some(style_name) = mat.get(1).map(|v| v.as_str().trim()) else {
            continue;
        };
        let Some(style_value) = mat.get(2).map(|v| v.as_str().trim()) else {
            continue;
        };

        if style_name.len() == 0 || style_value.len() == 0 {
            continue;
        }

        result
            .props
            .push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                KeyValueProp {
                    key: str_to_propname(style_name, span),
                    value: Box::from(Expr::Lit(Lit::Str(Str {
                        span,
                        value: style_value.into(),
                        raw: Some(style_value.into()),
                    }))),
                },
            ))));
    }

    result
}

/// Converts event names with dashes to camelcase identifiers,
/// e.g. `click` -> `onClick`, `state-changed` -> `onStateChanged`
fn event_name_to_handler(event_name: &str) -> JsWord {
    let mut result = String::with_capacity(event_name.len() + 2);
    result.push_str("on");

    // ignore error, idk what to do if writing to String fails
    let _ = to_pascalcase(event_name, &mut result);

    JsWord::from(result)
}

/// Generates () => {}
fn empty_arrow_expr(span: Span) -> Expr {
    Expr::Arrow(ArrowExpr {
        span,
        params: vec![],
        body: Box::from(BlockStmtOrExpr::BlockStmt(BlockStmt {
            span,
            stmts: vec![],
        })),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    })
}

#[cfg(test)]
mod tests {
    use fervid_core::{HtmlAttribute, VBindDirective, VDirective, VOnDirective};
    use swc_core::{
        common::DUMMY_SP,
        ecma::ast::ObjectLit,
    };

    use crate::context::CodegenContext;

    use super::DirectivesToProcess;

    #[test]
    fn it_generates_class_regular() {
        test_out(
            vec![HtmlAttribute::Regular {
                name: "class",
                value: "both regular and bound",
            }],
            r#"{class:"both regular and bound"}"#,
        );
    }

    #[test]
    fn it_generates_class_bound() {
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::Bind(
                VBindDirective {
                    argument: Some("class"),
                    value: "[item2, index]",
                    ..Default::default()
                },
            ))],
            r#"{class:_normalizeClass([_ctx.item2,_ctx.index])}"#,
        );
    }

    #[test]
    fn it_generates_both_classes() {
        test_out(
            vec![
                HtmlAttribute::Regular {
                    name: "class",
                    value: "both regular and bound",
                },
                HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                    argument: Some("class"),
                    value: "[item2, index]",
                    ..Default::default()
                })),
            ],
            r#"{class:_normalizeClass(["both regular and bound",[_ctx.item2,_ctx.index]])}"#,
        );
    }

    #[test]
    fn it_generates_style_regular() {
        test_out(
            vec![HtmlAttribute::Regular {
                name: "style",
                value: "margin: 0px; background-color: magenta",
            }],
            r#"{style:{margin:"0px","background-color":"magenta"}}"#,
        );
    }

    #[test]
    fn it_generates_style_bound() {
        // `:style="{ backgroundColor: v ? 'yellow' : undefined }"`
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::Bind(
                VBindDirective {
                    argument: Some("style"),
                    value: "{ backgroundColor: v ? 'yellow' : undefined }",
                    ..Default::default()
                },
            ))],
            r#"{style:_normalizeStyle({backgroundColor:_ctx.v?"yellow":undefined})}"#,
        );
    }

    #[test]
    fn it_generates_both_styles() {
        test_out(
            vec![
                HtmlAttribute::Regular {
                    name: "style",
                    value: "margin: 0px; background-color: magenta",
                },
                HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                    argument: Some("style"),
                    value: "{ backgroundColor: v ? 'yellow' : undefined }",
                    ..Default::default()
                })),
            ],
            r#"{style:_normalizeStyle([{margin:"0px","background-color":"magenta"},{backgroundColor:_ctx.v?"yellow":undefined}])}"#,
        );
    }

    #[test]
    fn it_generates_v_bind() {
        // :disabled="true"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::Bind(
                VBindDirective {
                    argument: Some("disabled"),
                    value: "true",
                    ..Default::default()
                },
            ))],
            "{disabled:true}",
        );

        // :multi-word-binding="true"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::Bind(
                VBindDirective {
                    argument: Some("multi-word-binding"),
                    value: "true",
                    ..Default::default()
                },
            ))],
            r#"{"multi-word-binding":true}"#,
        );

        // :disabled="some && expression || maybe !== not"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::Bind(
                VBindDirective {
                    argument: Some("disabled"),
                    value: "some && expression || maybe !== not",
                    ..Default::default()
                },
            ))],
            "{disabled:_ctx.some&&_ctx.expression||_ctx.maybe!==_ctx.not}",
        );
    }

    #[test]
    fn it_generates_v_on() {
        // @click
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("click"),
                handler: None,
                is_dynamic_event: false,
                modifiers: vec![],
            }))],
            r"{onClick:()=>{}}",
        );

        // @multi-word-event
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("multi-word-event"),
                handler: None,
                is_dynamic_event: false,
                modifiers: vec![],
            }))],
            r"{onMultiWordEvent:()=>{}}",
        );

        // @click="handleClick"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("click"),
                handler: Some("handleClick"),
                is_dynamic_event: false,
                modifiers: vec![],
            }))],
            r"{onClick:_ctx.handleClick}",
        );

        // TODO
        // @click="console.log('hello')"
        // test_out(
        //     vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
        //         event: Some("click"),
        //         handler: Some("() => console.log('hello')"),
        //         is_dynamic_event: false,
        //         modifiers: vec![],
        //     }))],
        //     r"{onClick:()=>console.log('hello')}"
        // );

        // @click="() => console.log('hello')"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("click"),
                handler: Some("() => console.log('hello')"),
                is_dynamic_event: false,
                modifiers: vec![],
            }))],
            r#"{onClick:()=>console.log("hello")}"#,
        );

        // @click="$event => handleClick($event, foo, bar)"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("click"),
                handler: Some("$event => handleClick($event, foo, bar)"),
                is_dynamic_event: false,
                modifiers: vec![],
            }))],
            // TODO Fix arrow function params transformation
            // r"{onClick:$event=>_ctx.handleClick($event,_ctx.foo,_ctx.bar)}"
            r"{onClick:$event=>_ctx.handleClick(_ctx.$event,_ctx.foo,_ctx.bar)}",
        );

        // @click.stop.prevent.self
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("click"),
                handler: None,
                is_dynamic_event: false,
                modifiers: vec!["stop", "prevent", "self"],
            }))],
            r#"{onClick:_withModifiers(()=>{},["stop","prevent","self"])}"#,
        );

        // @click.stop="$event => handleClick($event, foo, bar)"
        test_out(
            vec![HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                event: Some("click"),
                handler: Some("$event => handleClick($event, foo, bar)"),
                is_dynamic_event: false,
                modifiers: vec!["stop"],
            }))],
            // r#"{onClick:_withModifiers($event=>_ctx.handleClick($event,_ctx.foo,_ctx.bar),["stop"])}"#
            r#"{onClick:_withModifiers($event=>_ctx.handleClick(_ctx.$event,_ctx.foo,_ctx.bar),["stop"])}"#,
        );
    }

    fn test_out(input: Vec<HtmlAttribute>, expected: &str) {
        let mut ctx = CodegenContext::default();
        let mut out = ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        };
        let mut unsupported_directives = DirectivesToProcess::new();
        ctx.generate_attributes(&input, &mut out.props, &mut unsupported_directives, 0);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
