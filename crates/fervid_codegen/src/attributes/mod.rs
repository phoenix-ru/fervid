use fervid_core::{
    fervid_atom, AttributeOrBinding, FervidAtom, IntoIdent, StrOrExpr, VBindDirective,
    VOnDirective, VueImports,
};
use regex::Regex;
use swc_core::{
    common::{Span, Spanned, DUMMY_SP},
    ecma::ast::{
        ArrayLit, ArrowExpr, BinExpr, BinaryOp, BlockStmt, BlockStmtOrExpr, CallExpr, Callee,
        ComputedPropName, Expr, ExprOrSpread, Ident, IdentName, KeyValueProp, Lit, ObjectLit, Prop,
        PropName, PropOrSpread, Str,
    },
};

use crate::{context::CodegenContext, utils::str_to_propname};

lazy_static! {
    static ref CSS_RE: Regex =
        Regex::new(r"(?U)([a-zA-Z_-][a-zA-Z_0-9-]*):\s*(.+)(?:;|$)").unwrap();
}

/// Type alias for all the directives not handled as attributes.
/// Only `v-on` and `v-bind` as well as `v-model` for components generate attribute code.
/// Other directives have their own specifics of code generation, which are handled separately.
// pub type DirectivesToProcess<'i> = SmallVec<[&'i VDirective<'i>; 2]>;

#[derive(Debug, Default)]
pub struct GenerateAttributesResultHints<'i> {
    // _normalizeProps({
    //     foo: "bar",
    //     [_ctx.dynamic || ""]: _ctx.hi
    // })
    pub needs_normalize_props: bool,

    /// When `v-bind="smth"` was found
    pub v_bind_no_arg: Option<&'i VBindDirective>,

    /// When `v-on="smth"` was found
    pub v_on_no_event: Option<&'i VOnDirective>,

    /// When a js binding in :class was found
    pub class_patch_flag: bool,

    /// When a js binding in :style was found
    pub style_patch_flag: bool,

    /// When a prop other than `class` or `style` has a js binding
    pub props_patch_flag: bool,
}

impl CodegenContext {
    pub fn generate_attributes<'attr>(
        &mut self,
        attributes: &'attr [AttributeOrBinding],
        out: &mut Vec<PropOrSpread>,
    ) -> GenerateAttributesResultHints<'attr> {
        // Special generation for `class` and `style` attributes,
        // as they can have both Regular and VDirective variants
        let mut class_regular_attr: Option<(&FervidAtom, Span)> = None;
        let mut class_bound: Option<(Box<Expr>, Span)> = None;
        let mut style_regular_attr: Option<(&FervidAtom, Span)> = None;
        let mut style_bound: Option<(Box<Expr>, Span)> = None;

        // Hints on what was processed and what to do next
        let mut result_hints = GenerateAttributesResultHints::default();

        for attribute in attributes {
            match attribute {
                // First, we check the special case: `class` and `style` attributes
                // class
                AttributeOrBinding::RegularAttribute { name, value, span } if name == "class" => {
                    class_regular_attr = Some((value, *span));
                }

                // style
                AttributeOrBinding::RegularAttribute { name, value, span } if name == "style" => {
                    style_regular_attr = Some((value, *span));
                }

                // Any regular attribute will be added as an object entry,
                // where key is attribute name and value is attribute value as string literal
                AttributeOrBinding::RegularAttribute { name, value, span } => {
                    // let raw = Some(Atom::from(value.as_ref()));

                    out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                        KeyValueProp {
                            key: str_to_propname(&name, *span),
                            value: Box::from(Expr::Lit(Lit::Str(Str {
                                span: span.to_owned(),
                                value: value.to_owned(),
                                raw: None,
                            }))),
                        },
                    ))));
                }

                // Directive.
                // `v-on` and `v-bind` are processed here, other directives
                // will be added to the vector of unprocessed directives

                // :class
                AttributeOrBinding::VBind(VBindDirective {
                    argument: Some(StrOrExpr::Str(argument)),
                    value,
                    span,
                    ..
                }) if argument == "class" => {
                    class_bound = Some((value.to_owned(), *span));
                }

                // :style
                AttributeOrBinding::VBind(VBindDirective {
                    argument: Some(StrOrExpr::Str(argument)),
                    value,
                    span,
                    ..
                }) if argument == "style" => {
                    style_bound = Some((value.to_owned(), *span));
                }

                // `v-bind` directive without argument needs its own processing
                AttributeOrBinding::VBind(v_bind) if v_bind.argument.is_none() => {
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
                AttributeOrBinding::VOn(v_on) if v_on.event.is_none() => {
                    result_hints.v_on_no_event = Some(v_on);
                }

                // `v-bind` directive, shortcut `:`, e.g. `:custom-prop="value"`
                AttributeOrBinding::VBind(VBindDirective {
                    argument: Some(argument),
                    value,
                    span,
                    ..
                }) => {
                    // Transform the raw expression
                    // let was_transformed =
                    //     transform_scoped(&mut value, &self.scope_helper, template_scope_id);
                    let was_transformed = true; // todo
                    let span = *span;

                    // Add the PROPS patch flag
                    result_hints.props_patch_flag =
                        result_hints.props_patch_flag || was_transformed;

                    let key = match argument {
                        StrOrExpr::Str(s) => str_to_propname(s, span),
                        StrOrExpr::Expr(expr) => {
                            // Dynamic prop needs a `_normalizeProps` call
                            // TODO Take from patch flags?
                            result_hints.needs_normalize_props = true;

                            // `[key_transformed || ""]`
                            PropName::Computed(ComputedPropName {
                                span,
                                expr: Box::from(Expr::Bin(BinExpr {
                                    span,
                                    op: BinaryOp::LogicalOr,
                                    left: expr.to_owned(), // ?
                                    right: Box::from(Expr::Lit(Lit::Str(Str {
                                        span,
                                        value: FervidAtom::from(""),
                                        raw: None,
                                    }))),
                                })),
                            })
                        }
                    };

                    out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                        KeyValueProp {
                            key,
                            value: value.to_owned(), // ?
                        },
                    ))));
                }

                // v-on directive, shortcut `@`, e.g. `@custom-event.modifier="value"`
                AttributeOrBinding::VOn(VOnDirective {
                    event: Some(event),
                    handler,
                    modifiers,
                    span,
                }) => {
                    // TODO Use _cache
                    let span = *span;

                    // Transform or default to () => {}
                    // The patch flag does not apply to v-on
                    // TODO Empty `v-on` should be handled using `mergeProps` and `toHandlers`
                    let handler = handler
                        .to_owned()
                        .unwrap_or_else(|| Box::new(empty_arrow_expr(span)));

                    // To generate as an array of `["modifier1", "modifier2"]`
                    let modifiers: Vec<Option<ExprOrSpread>> = modifiers
                        .iter()
                        .map(|modifier| {
                            Some(ExprOrSpread {
                                spread: None,
                                expr: Box::from(Expr::Lit(Lit::Str(Str {
                                    span,
                                    value: modifier.to_owned(),
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
                            ctxt: Default::default(),
                            callee: Callee::Expr(Box::from(Expr::Ident(
                                with_modifiers_import.into_ident_spanned(span),
                            ))),
                            args: vec![
                                ExprOrSpread {
                                    expr: handler,
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
                        handler
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

                    // IDEA: Do the transformation beforehand, and put resulting `Expr`s in the return struct

                    match event {
                        StrOrExpr::Str(event_name_str) => {
                            // e.g. `onClick: _ctx.handleClick` or `onClick: _withModifiers(() => {}, ["stop"])
                            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                                KeyValueProp {
                                    key: str_to_propname(&event_name_str, span),
                                    value: handler_expr,
                                },
                            ))));
                        }

                        // TODO Instead of pushing to `out`, signify that `mergeProps` and `toHandlerKey` are needed
                        StrOrExpr::Expr(event_name_expr) => {
                            out.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                                key: PropName::Computed(ComputedPropName {
                                    span: DUMMY_SP,
                                    expr: event_name_expr.to_owned(),
                                }),
                                value: handler_expr,
                            }))));
                        }
                    }
                }

                _ => unreachable!(),
            }
        }

        result_hints.class_patch_flag =
            self.generate_class_bindings(class_regular_attr, class_bound, out);
        result_hints.style_patch_flag =
            self.generate_style_bindings(style_regular_attr, style_bound, out);

        result_hints
    }

    /// Process `class` attribute. We may have a regular one, a bound one, both or neither.
    /// Returns `true` when there were JavaScript bindings
    fn generate_class_bindings(
        &mut self,
        class_regular_attr: Option<(&FervidAtom, Span)>,
        class_bound: Option<(Box<Expr>, Span)>,
        out: &mut Vec<PropOrSpread>,
    ) -> bool {
        let mut expr: Option<Expr> = None;
        let mut has_js_bindings = false;

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
                        value: regular_value.to_owned(),
                        raw: None, //Some(Atom::from(regular_value.as_ref())),
                    }))),
                }));

                // 3. Transform the bound value
                // let was_transformed =
                //     transform_scoped(&mut bound_value, &self.scope_helper, scope_to_use);
                let was_transformed = true; // TODO

                // 4. ["regular classes", boundClasses]
                normalize_array.elems.push(Some(ExprOrSpread {
                    spread: None,
                    expr: bound_value,
                }));

                // `normalizeClass(["regular classes", boundClasses])`
                expr = Some(Expr::Call(CallExpr {
                    span: bound_span,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::from(Expr::Ident(
                        self.get_and_add_import_ident(VueImports::NormalizeClass)
                            .into_ident_spanned(bound_span),
                    ))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::from(Expr::Array(normalize_array)),
                    }],
                    type_args: None,
                }));

                has_js_bindings = was_transformed;
            }

            // Just regular `class`
            (Some((regular_value, span)), None) => {
                expr = Some(Expr::Lit(Lit::Str(Str {
                    raw: None, // Some(Atom::from(regular_value.as_ref())),
                    value: regular_value.to_owned(),
                    span,
                })));
            }

            // Just bound `:class`
            (None, Some((bound_value, span))) => {
                // let was_transformed =
                //     transform_scoped(&mut bound_value, &self.scope_helper, scope_to_use);
                let was_transformed = true; // TODO

                // `normalizeClass(boundClasses)`
                expr = Some(Expr::Call(CallExpr {
                    span,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::from(Expr::Ident(
                        self.get_and_add_import_ident(VueImports::NormalizeClass)
                            .into_ident_spanned(span),
                    ))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: bound_value,
                    }],
                    type_args: None,
                }));

                has_js_bindings = was_transformed;
            }

            // Neither
            (None, None) => {}
        }

        // Add `class` to attributes
        if let Some(expr) = expr {
            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                KeyValueProp {
                    key: PropName::Ident(IdentName {
                        sym: fervid_atom!("class"),
                        span: expr.span(),
                    }),
                    value: Box::from(expr),
                },
            ))));
        }

        has_js_bindings
    }

    /// Process `style` attribute. We may have a regular one, a bound one, both or neither.
    /// Returns `true` when there were JavaScript bindings
    fn generate_style_bindings(
        &mut self,
        style_regular_attr: Option<(&FervidAtom, Span)>,
        style_bound: Option<(Box<Expr>, Span)>,
        out: &mut Vec<PropOrSpread>,
    ) -> bool {
        let mut expr = None;
        let mut has_js_bindings = false;

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
                // let was_transformed =
                //     transform_scoped(&mut bound_value, &self.scope_helper, scope_to_use);
                let was_transformed = true; // TODO

                // 5. [{ regular: "styles as an object" }, boundStyles]
                normalize_array.elems.push(Some(ExprOrSpread {
                    spread: None,
                    expr: bound_value, // ?
                }));

                // `normalizeClass([{ regular: "styles as an object" }, boundStyles])`
                expr = Some(Expr::Call(CallExpr {
                    span: bound_span,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::from(Expr::Ident(
                        self.get_and_add_import_ident(VueImports::NormalizeStyle)
                            .into_ident_spanned(bound_span),
                    ))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::from(Expr::Array(normalize_array)),
                    }],
                    type_args: None,
                }));

                has_js_bindings = was_transformed;
            }

            // `style`
            (Some((regular_value, span)), None) => {
                expr = Some(Expr::Object(generate_regular_style(regular_value, span)));
            }

            // `:style`
            (None, Some((bound_value, span))) => {
                // let was_transformed =
                //     transform_scoped(&mut bound_value, &self.scope_helper, scope_to_use);
                let was_transformed = true; // TODO

                // `normalizeStyle(boundStyles)`
                expr = Some(Expr::Call(CallExpr {
                    span,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::from(Expr::Ident(Ident {
                        span,
                        ctxt: Default::default(),
                        sym: self.get_and_add_import_ident(VueImports::NormalizeStyle),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: bound_value,
                    }],
                    type_args: None,
                }));

                has_js_bindings = was_transformed;
            }

            (None, None) => {}
        }

        // Add `style` to attributes
        if let Some(expr) = expr {
            out.push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                KeyValueProp {
                    key: PropName::Ident(IdentName {
                        sym: fervid_atom!("style"),
                        span: expr.span(),
                    }),
                    value: Box::from(expr),
                },
            ))));
        }

        has_js_bindings
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
                        raw: None, // Some(style_value.into()),
                    }))),
                },
            ))));
    }

    result
}

/// Generates () => {}
fn empty_arrow_expr(span: Span) -> Expr {
    Expr::Arrow(ArrowExpr {
        span,
        ctxt: Default::default(),
        params: vec![],
        body: Box::from(BlockStmtOrExpr::BlockStmt(BlockStmt {
            span,
            ctxt: Default::default(),
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
    use fervid_core::{AttributeOrBinding, VOnDirective};
    use swc_core::{common::DUMMY_SP, ecma::ast::ObjectLit};

    use crate::{
        context::CodegenContext,
        test_utils::{js, regular_attribute, v_bind_attribute, v_on_attribute},
    };

    #[test]
    fn it_generates_class_regular() {
        test_out(
            vec![regular_attribute("class", "both regular and bound")],
            r#"{class:"both regular and bound"}"#,
        );
    }

    #[test]
    fn it_generates_class_bound() {
        test_out(
            vec![v_bind_attribute("class", "[item2, index]")],
            r#"{class:_normalizeClass([item2,index])}"#,
        );
    }

    #[test]
    fn it_generates_both_classes() {
        test_out(
            vec![
                regular_attribute("class", "both regular and bound"),
                v_bind_attribute("class", "[item2, index]"),
            ],
            r#"{class:_normalizeClass(["both regular and bound",[item2,index]])}"#,
        );
    }

    #[test]
    fn it_generates_style_regular() {
        test_out(
            vec![regular_attribute(
                "style",
                "margin: 0px; background-color: magenta",
            )],
            r#"{style:{margin:"0px","background-color":"magenta"}}"#,
        );
    }

    #[test]
    fn it_generates_style_bound() {
        // `:style="{ backgroundColor: v ? 'yellow' : undefined }"`
        test_out(
            vec![v_bind_attribute(
                "style",
                "{ backgroundColor: v ? 'yellow' : undefined }",
            )],
            r#"{style:_normalizeStyle({backgroundColor:v?"yellow":undefined})}"#,
        );
    }

    #[test]
    fn it_generates_both_styles() {
        test_out(
            vec![
                regular_attribute("style", "margin: 0px; background-color: magenta"),
                v_bind_attribute("style", "{ backgroundColor: v ? 'yellow' : undefined }"),
            ],
            r#"{style:_normalizeStyle([{margin:"0px","background-color":"magenta"},{backgroundColor:v?"yellow":undefined}])}"#,
        );
    }

    #[test]
    fn it_generates_v_bind() {
        // :disabled="true"
        test_out(
            vec![v_bind_attribute("disabled", "true")],
            "{disabled:true}",
        );

        // :multi-word-binding="true"
        test_out(
            vec![v_bind_attribute("multi-word-binding", "true")],
            r#"{"multi-word-binding":true}"#,
        );

        // :disabled="some && expression || maybe !== not"
        test_out(
            vec![v_bind_attribute(
                "disabled",
                "some && expression || maybe !== not",
            )],
            "{disabled:some&&expression||maybe!==not}",
        );
    }

    #[test]
    fn it_generates_v_on() {
        // @click
        test_out(
            vec![AttributeOrBinding::VOn(VOnDirective {
                event: Some("onClick".into()),
                handler: None,
                modifiers: vec![],
                span: DUMMY_SP,
            })],
            r"{onClick:()=>{}}",
        );

        // @multi-word-event (gets transformed to `onMultiWordEvent`)
        test_out(
            vec![AttributeOrBinding::VOn(VOnDirective {
                event: Some("onMultiWordEvent".into()),
                handler: None,
                modifiers: vec![],
                span: DUMMY_SP,
            })],
            r"{onMultiWordEvent:()=>{}}",
        );

        // @click="handleClick"
        test_out(
            vec![v_on_attribute("onClick", "handleClick")],
            r"{onClick:handleClick}",
        );

        // TODO
        // This should have been transformed previously
        // @click="console.log('hello')"
        // test_out(
        //     vec![AttributeOrBinding::VOn(VOnDirective {
        //         event: Some("click".into()),
        //         handler: Some(js("() => console.log('hello')")),
        //         modifiers: vec![],
        //         span: DUMMY_SP
        //     })],
        //     r"{onClick:()=>console.log('hello')}"
        // );

        // @click="() => console.log('hello')"
        test_out(
            vec![v_on_attribute("onClick", "() => console.log('hello')")],
            r#"{onClick:()=>console.log("hello")}"#,
        );

        // @click="$event => handleClick($event, foo, bar)"
        test_out(
            vec![v_on_attribute(
                "onClick",
                "$event => handleClick($event, foo, bar)",
            )],
            r"{onClick:$event=>handleClick($event,foo,bar)}",
        );

        // @click.stop.prevent.self
        test_out(
            vec![AttributeOrBinding::VOn(VOnDirective {
                event: Some("onClick".into()),
                handler: None,
                modifiers: vec!["stop".into(), "prevent".into(), "self".into()],
                span: DUMMY_SP,
            })],
            r#"{onClick:_withModifiers(()=>{},["stop","prevent","self"])}"#,
        );

        // @click.stop="$event => handleClick($event, foo, bar)"
        test_out(
            vec![AttributeOrBinding::VOn(VOnDirective {
                event: Some("onClick".into()),
                handler: Some(js("$event => handleClick($event, foo, bar)")),
                modifiers: vec!["stop".into()],
                span: DUMMY_SP,
            })],
            r#"{onClick:_withModifiers($event=>handleClick($event,foo,bar),["stop"])}"#,
        );
    }

    fn test_out(input: Vec<AttributeOrBinding>, expected: &str) {
        let mut ctx = CodegenContext::default();
        let mut out = ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        };
        ctx.generate_attributes(&input, &mut out.props);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
