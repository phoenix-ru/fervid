use fervid_core::{
    fervid_atom, BindingTypes, BindingsHelper, FervidAtom, PatchFlags, PatchHints, SetupBinding,
    StrOrExpr, TemplateGenerationMode, VModelDirective, VueImports,
};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::{
        ast::{
            ArrowExpr, AssignExpr, AssignOp, BindingIdent, BlockStmtOrExpr, CallExpr, Callee,
            CondExpr, Expr, ExprOrSpread, Ident, KeyValueProp, Lit, MemberExpr, MemberProp, Null,
            Pat, PatOrExpr, Prop, PropName, PropOrSpread,
        },
        atoms::JsWord,
        visit::{VisitMut, VisitMutWith},
    },
};

use crate::{script::common::extract_variables_from_pat, template::js_builtins::JS_BUILTINS};

struct TransformVisitor<'s> {
    current_scope: u32,
    bindings_helper: &'s mut BindingsHelper,
    has_js_bindings: bool,
    is_inline: bool,
    // `SetupBinding` instead of `FervidAtom` to easier interface with `extract_variables_from_pat`
    local_vars: Vec<SetupBinding>,
}

pub trait BindingsHelperTransform {
    fn transform_expr(&mut self, expr: &mut Expr, scope_to_use: u32) -> bool;
    fn transform_v_model(
        &mut self,
        v_model: &mut VModelDirective,
        scope_to_use: u32,
        patch_hints: &mut PatchHints,
    );
    fn get_var_binding_type(&mut self, starting_scope: u32, variable: &FervidAtom) -> BindingTypes;
}

impl BindingsHelperTransform for BindingsHelper {
    /// Transforms the template expression
    fn transform_expr(&mut self, expr: &mut Expr, scope_to_use: u32) -> bool {
        let is_inline = matches!(
            self.template_generation_mode,
            TemplateGenerationMode::Inline
        );
        let mut visitor = TransformVisitor {
            current_scope: scope_to_use,
            bindings_helper: self,
            has_js_bindings: false,
            is_inline,
            local_vars: Vec::new(),
        };
        expr.visit_mut_with(&mut visitor);

        visitor.has_js_bindings
    }

    /// Transforms `v-model` directive by producing
    /// `:value` expression and
    /// `@update:value` handler (`$event => modelValue = $event`).
    fn transform_v_model(
        &mut self,
        v_model: &mut VModelDirective,
        scope_to_use: u32,
        patch_hints: &mut PatchHints,
    ) {
        // 1. Create handler: wrap in `$event => value = $event`
        let event_expr = Box::new(Expr::Ident(Ident {
            span: DUMMY_SP,
            sym: FervidAtom::from("$event"),
            optional: false,
        }));
        let mut handler =
            wrap_in_event_arrow(wrap_in_assignment(v_model.value.to_owned(), event_expr));

        // 2. Transform handler
        self.transform_expr(&mut handler, scope_to_use);

        // 3. Assign handler
        v_model.update_handler = Some(handler);

        // 4. Transform value
        self.transform_expr(&mut v_model.value, scope_to_use);

        // 5. (Optional) Transform dynamic argument and set patch hints
        match v_model.argument {
            Some(StrOrExpr::Expr(ref mut expr)) => {
                self.transform_expr(expr, scope_to_use);
                patch_hints.flags |= PatchFlags::FullProps;
            }

            Some(StrOrExpr::Str(ref argument)) => {
                patch_hints.flags |= PatchFlags::Props;
                patch_hints.props.push(argument.to_owned());
            }

            None => {
                patch_hints.flags |= PatchFlags::Props;
                patch_hints.props.push(fervid_atom!("modelValue"));
            }
        }

        // TODO Check that SetupConst or SetupReactiveConst are not used as a `v-model` value. Report hard error in this case.
        // TODO Check in general in all cases
    }

    fn get_var_binding_type(&mut self, starting_scope: u32, variable: &FervidAtom) -> BindingTypes {
        if JS_BUILTINS.contains(variable) {
            return BindingTypes::JsGlobal;
        }

        let mut current_scope_index = starting_scope;

        // Check template scope
        while let Some(current_scope) = self.template_scopes.get(current_scope_index as usize) {
            // Check variable existence in the current scope
            let found = current_scope.variables.iter().find(|it| *it == variable);

            if let Some(_) = found {
                return BindingTypes::TemplateLocal;
            }

            // Check if we reached the root scope, it will have itself as a parent
            if current_scope.parent == current_scope_index {
                break;
            }

            // Go to parent
            current_scope_index = current_scope.parent;
        }

        // Check hash-map for convenience (we may have found the reference previously)
        let variable_atom = variable.to_owned();
        if let Some(binding_type) = self.used_bindings.get(&variable_atom) {
            return binding_type.to_owned();
        }

        // Check setup bindings (both `<script setup>` and `setup()`)
        let setup_bindings = self.setup_bindings.iter().chain(
            self.options_api_bindings
                .as_ref()
                .map_or_else(|| [].iter(), |v| v.setup.iter()),
        );
        for binding in setup_bindings {
            if binding.0 == variable_atom {
                self.used_bindings.insert(variable_atom, binding.1);
                return binding.1;
            }
        }

        // Macro to check if the variable is in the slice/Vec and conditionally return
        macro_rules! check_scope {
            ($vars: expr, $ret_descriptor: expr) => {
                if $vars.iter().any(|it| *it == variable_atom) {
                    self.used_bindings.insert(variable_atom, $ret_descriptor);
                    return $ret_descriptor;
                }
            };
        }

        // Check all the options API variables
        if let Some(options_api_vars) = &self.options_api_bindings {
            check_scope!(options_api_vars.data, BindingTypes::Data);
            check_scope!(options_api_vars.props, BindingTypes::Props);
            check_scope!(options_api_vars.computed, BindingTypes::Options);
            check_scope!(options_api_vars.methods, BindingTypes::Options);
            check_scope!(options_api_vars.inject, BindingTypes::Options);

            // Check options API imports.
            // Currently it ignores the SyntaxContext (same as in js implementation)
            for binding in options_api_vars.imports.iter() {
                if binding.0 == variable_atom {
                    self.used_bindings
                        .insert(variable_atom, BindingTypes::SetupMaybeRef);
                    return BindingTypes::SetupMaybeRef;
                }
            }
        }

        BindingTypes::Unresolved
    }
}

impl<'s> VisitMut for TransformVisitor<'s> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        let ident_expr: &mut Ident = match n {
            // Special treatment for assignment expression
            Expr::Assign(assign_expr) => {
                // Visit RHS first
                assign_expr.right.visit_mut_with(self);

                // Check for special case: LHS is an ident of type `SetupLet`
                // This is only valid for Inline mode
                let setup_let_ident = if self.is_inline {
                    let ident = match assign_expr.left {
                        PatOrExpr::Expr(ref e) => e.as_ident(),

                        PatOrExpr::Pat(ref pat) => match **pat {
                            Pat::Ident(ref binding_ident) => Some(&binding_ident.id),
                            Pat::Expr(ref e) => e.as_ident(),
                            _ => None,
                        },
                    };

                    ident.and_then(|ident| {
                        let binding_type = self
                            .bindings_helper
                            .get_var_binding_type(self.current_scope, &ident.sym);

                        if let BindingTypes::SetupLet | BindingTypes::SetupMaybeRef = binding_type {
                            Some((ident, binding_type))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };

                // Special case for `SetupLet` or `SetupMaybeRef`: generate `isRef` check
                if let Some((ident, binding_type)) = setup_let_ident {
                    // SetupMaybeRef is constant, reassignment is not possible
                    let is_reassignable = matches!(binding_type, BindingTypes::SetupLet);

                    *n = *generate_is_ref_check_assignment(
                        ident,
                        &assign_expr.right,
                        self.bindings_helper,
                        is_reassignable,
                    );
                } else {
                    assign_expr.left.visit_mut_with(self);
                }

                return;
            }

            // Arrow functions need params collection
            Expr::Arrow(arrow_expr) => {
                let old_len = self.local_vars.len();

                // Add the temporary variables
                self.local_vars.reserve(arrow_expr.params.len());
                for param in arrow_expr.params.iter() {
                    extract_variables_from_pat(param, &mut self.local_vars, true);
                }

                // Transform the arrow body
                arrow_expr.body.visit_mut_with(self);

                // Clear the temporary variables
                self.local_vars.drain(old_len..);

                return;
            }

            // Identifier is what we need for the rest of the function
            Expr::Ident(ident_expr) => ident_expr,

            _ => {
                n.visit_mut_children_with(self);
                return;
            }
        };

        let symbol = &ident_expr.sym;
        let span = ident_expr.span;

        // Try to find variable in the local vars (e.g. arrow function params)
        if let Some(_) = self.local_vars.iter().rfind(|it| &it.0 == symbol) {
            self.has_js_bindings = true;
            return;
        }

        let binding_type = self
            .bindings_helper
            .get_var_binding_type(self.current_scope, symbol);

        // Template local binding doesn't need any processing
        if let BindingTypes::TemplateLocal = binding_type {
            self.has_js_bindings = true;
            return;
        }

        // Get the prefix which fits the scope (e.g. `_ctx.` for unknown scopes, `$setup.` for setup scope)
        if let Some(prefix) = get_prefix(&binding_type, self.is_inline) {
            *n = Expr::Member(MemberExpr {
                span,
                obj: Box::new(Expr::Ident(Ident {
                    span,
                    sym: prefix,
                    optional: false,
                })),
                prop: MemberProp::Ident(ident_expr.to_owned()),
            });
            self.has_js_bindings = true;
        }

        // Non-inline logic ends here
        if !self.is_inline {
            return;
        }

        // TODO The logic for setup variables actually differs quite significantly
        // https://play.vuejs.org/#eNp9UU1rwzAM/SvCl25QEkZvIRTa0cN22Mq6oy8hUVJ3iW380QWC//tkh2Y7jN6k956kJ2liO62zq0dWsNLWRmgHFp3XWy7FoJVxMMEOArRGDbDK8v2KywZbIfFolLYPE5cArVIFnJwRsuMyPHJZ5nMv6kKJw0H3lUPKAMrz03aaYgmEQAE1D2VOYKxalGzNnK2VbEWXXaySZC9N4qxWgxY9mnfthJKWswISE7mq79X3a8Kc8bi+4fUZ669/8IsdI8bZ0aBFc0XOFs5VpkM304fTG44UL+SgGt+T+g75gVb1PnqcZXsvG7L9R5fcvqQj0+E+7WF0KO1tqWg0KkPSc0Y/er6z+q/dTbZJdfQJFn4A+DKelw==

        let dot_value = |expr: &mut Expr, span: Span| {
            *expr = Expr::Member(MemberExpr {
                span,
                obj: Box::new(expr.to_owned()),
                prop: MemberProp::Ident(Ident {
                    span: DUMMY_SP,
                    sym: "value".into(),
                    optional: false,
                }),
            })
        };

        let mut unref = |expr: &mut Expr, span: Span| {
            self.bindings_helper.vue_imports |= VueImports::Unref;

            *expr = Expr::Call(CallExpr {
                span,
                callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                    span,
                    sym: VueImports::Unref.as_atom(),
                    optional: false,
                }))),
                args: vec![ExprOrSpread {
                    spread: None,
                    expr: Box::new(expr.to_owned()),
                }],
                type_args: None,
            });
        };

        // Add a flag that binding is dynamic
        if matches!(
            binding_type,
            BindingTypes::SetupLet
                | BindingTypes::SetupReactiveConst
                | BindingTypes::SetupMaybeRef
                | BindingTypes::SetupRef
        ) {
            self.has_js_bindings = true;
        }

        // Inline logic is pretty complex
        // TODO Actual logic
        match binding_type {
            BindingTypes::SetupLet => unref(n, span),
            BindingTypes::SetupConst => {}
            BindingTypes::SetupReactiveConst => {}
            BindingTypes::SetupMaybeRef => unref(n, span),
            BindingTypes::SetupRef => dot_value(n, span),
            BindingTypes::LiteralConst => {}
            _ => {}
        }
    }

    // fn visit_mut_ident(&mut self, n: &mut swc_core::ecma::ast::Ident) {
    //     let symbol = &n.sym;
    //     let scope = self.scope_helper.find_scope_of_variable(self.current_scope, symbol);

    //     let prefix = scope.get_prefix();
    //     if prefix.len() > 0 {
    //         let mut new_symbol = String::with_capacity(symbol.len() + prefix.len());
    //         new_symbol.push_str(prefix);
    //         new_symbol.push_str(&symbol);
    //         n.sym = new_symbol.into();
    //     }
    // }

    fn visit_mut_member_expr(&mut self, n: &mut swc_core::ecma::ast::MemberExpr) {
        if n.obj.is_ident() {
            n.obj.visit_mut_with(self)
        } else {
            n.visit_mut_children_with(self);
        }
    }

    fn visit_mut_object_lit(&mut self, n: &mut swc_core::ecma::ast::ObjectLit) {
        for prop in n.props.iter_mut() {
            match prop {
                PropOrSpread::Prop(ref mut prop) => {
                    // For shorthand, expand it and visit the value part
                    if let Some(shorthand) = prop.as_mut_shorthand() {
                        let prop_name = PropName::Ident(shorthand.to_owned());

                        let mut value_expr = Expr::Ident(shorthand.to_owned());
                        value_expr.visit_mut_with(self);

                        *prop = Prop::KeyValue(KeyValueProp {
                            key: prop_name,
                            value: Box::new(value_expr),
                        })
                        .into();
                        self.has_js_bindings = true;
                    } else if let Some(keyvalue) = prop.as_mut_key_value() {
                        keyvalue.value.visit_mut_with(self);
                    }
                }

                PropOrSpread::Spread(ref mut spread) => {
                    spread.visit_mut_with(self);
                }
            }
        }
    }
}

/// Gets the variable prefix depending on if we are compiling the template in inline mode.
/// This is used for transformations.
/// ## Example
/// `data()` variable `foo` in non-inline compilation becomes `$data.foo`.\
/// `setup()` ref variable `bar` in non-inline compilation becomes `$setup.bar`,
/// but in the inline compilation it remains the same.
pub fn get_prefix(binding_type: &BindingTypes, is_inline: bool) -> Option<JsWord> {
    // For inline mode, options API variables become prefixed
    if is_inline {
        return match binding_type {
            BindingTypes::Data | BindingTypes::Options | BindingTypes::Unresolved => {
                Some(FervidAtom::from("_ctx"))
            }
            BindingTypes::Props => Some(FervidAtom::from("__props")),
            // TODO This is not correct. The transform implementation must handle `unref`
            _ => None,
        };
    }

    match binding_type {
        BindingTypes::Data => Some(FervidAtom::from("$data")),
        BindingTypes::Props => Some(FervidAtom::from("$props")),
        BindingTypes::Options => Some(FervidAtom::from("$options")),
        BindingTypes::TemplateLocal | BindingTypes::JsGlobal | BindingTypes::LiteralConst => None,
        BindingTypes::SetupConst
        | BindingTypes::SetupLet
        | BindingTypes::SetupMaybeRef
        | BindingTypes::SetupReactiveConst
        | BindingTypes::SetupRef => Some(FervidAtom::from("$setup")),
        BindingTypes::Unresolved => Some(FervidAtom::from("_ctx")),
        BindingTypes::PropsAliased => unimplemented!(),
    }
}

/// Generates `_isRef(ident) ? (ident).value = rhs_expr : ident = rhs_expr`
fn generate_is_ref_check_assignment(
    lhs_ident: &Ident,
    rhs_expr: &Expr,
    bindings_helper: &mut BindingsHelper,
    is_reassignable: bool,
) -> Box<Expr> {
    // Get `isRef` helper
    let is_ref_ident = VueImports::IsRef.as_atom();
    bindings_helper.vue_imports |= VueImports::IsRef;

    // `ident` expression
    let ident_expr = Box::new(Expr::Ident(lhs_ident.to_owned()));

    // `isRef(ident)`
    let condition = Box::new(Expr::Call(CallExpr {
        span: DUMMY_SP,
        callee: Callee::Expr(Box::new(Expr::Ident(Ident {
            span: DUMMY_SP,
            sym: is_ref_ident,
            optional: false,
        }))),
        args: vec![ExprOrSpread {
            spread: None,
            expr: ident_expr.to_owned(),
        }],
        type_args: None,
    }));

    // `ident.value`
    let ident_dot_value = Box::new(Expr::Member(MemberExpr {
        span: DUMMY_SP,
        obj: ident_expr.to_owned(),
        prop: MemberProp::Ident(Ident {
            span: DUMMY_SP,
            sym: FervidAtom::from("value"),
            optional: false,
        }),
    }));

    // `ident.value = rhs_expr`
    let positive_assign = wrap_in_assignment(ident_dot_value, Box::new(rhs_expr.to_owned()));

    // `ident = rhs_expr` or `null`
    let negative_assign = if is_reassignable {
        wrap_in_assignment(ident_expr, Box::new(rhs_expr.to_owned()))
    } else {
        Box::new(Expr::Lit(Lit::Null(Null { span: DUMMY_SP })))
    };

    Box::new(Expr::Cond(CondExpr {
        span: DUMMY_SP,
        test: condition,
        cons: positive_assign,
        alt: negative_assign,
    }))
}

/// Wraps `expr` to `$event => (expr)`
#[inline]
fn wrap_in_event_arrow(expr: Box<Expr>) -> Box<Expr> {
    let evt_param = Pat::Ident(BindingIdent {
        id: Ident {
            span: DUMMY_SP,
            sym: FervidAtom::from("$event"),
            optional: false,
        },
        type_ann: None,
    });

    Box::new(Expr::Arrow(ArrowExpr {
        span: DUMMY_SP,
        params: vec![evt_param],
        body: Box::new(BlockStmtOrExpr::Expr(expr)),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    }))
}

/// Wraps `expr` to `expr = $event`
#[inline]
fn wrap_in_assignment(expr: Box<Expr>, rhs_expr: Box<Expr>) -> Box<Expr> {
    Box::new(Expr::Assign(AssignExpr {
        span: DUMMY_SP,
        op: AssignOp::Assign,
        left: PatOrExpr::Expr(expr),
        right: rhs_expr,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{
        template::{expr_transform::BindingsHelperTransform, js_builtins::JS_BUILTINS},
        test_utils::{parser::parse_javascript_expr, to_str},
    };
    use fervid_core::{
        BindingTypes, BindingsHelper, FervidAtom, PatchHints, SetupBinding, StrOrExpr,
        TemplateGenerationMode, TemplateScope, VModelDirective,
    };
    use smallvec::SmallVec;
    use swc_core::common::DUMMY_SP;

    #[test]
    fn it_acknowledges_builtins() {
        let mut helper = BindingsHelper::default();

        for builtin in JS_BUILTINS.iter() {
            assert_eq!(
                BindingTypes::JsGlobal,
                helper.get_var_binding_type(0, &FervidAtom::from(*builtin))
            );
        }

        // Check inline mode as well
        helper.template_generation_mode = TemplateGenerationMode::Inline;
        for builtin in JS_BUILTINS.iter() {
            assert_eq!(
                BindingTypes::JsGlobal,
                helper.get_var_binding_type(0, &FervidAtom::from(*builtin))
            );
        }
    }

    #[test]
    fn it_acknowledges_local_vars() {
        let mut helper = BindingsHelper::default();

        macro_rules! test {
            ($expr: literal, $expected: literal) => {
                let mut expr = js($expr);
                helper.transform_expr(&mut expr, 0);

                assert_eq!(to_str(&expr), $expected);
            };
        }

        test!(
            "$event => console.log($event)",
            "$event=>console.log($event)"
        );
        test!(
            "$event => console.log($event, foo)",
            "$event=>console.log($event,_ctx.foo)"
        );
        test!("x => y => doSmth(x, y, z)", "x=>y=>_ctx.doSmth(x,y,_ctx.z)");
        test!(
            "(x => doSmth(x), y => doSmth(x))",
            "(x=>_ctx.doSmth(x),y=>_ctx.doSmth(_ctx.x))"
        );
        test!(
            "(x => doSmth(x), () => doSmth(x))",
            "(x=>_ctx.doSmth(x),()=>_ctx.doSmth(_ctx.x))"
        );
    }

    #[test]
    fn it_transforms_v_model() {
        let mut helper = BindingsHelper::default();

        // const Ref = ref('foo')
        // const MaybeRef = useSomething()
        // const Const = 123
        // let Let = 123
        // const Reactive = reactive({})
        helper.setup_bindings.push(SetupBinding(
            FervidAtom::from("Ref"),
            BindingTypes::SetupRef,
        ));
        helper.setup_bindings.push(SetupBinding(
            FervidAtom::from("MaybeRef"),
            BindingTypes::SetupMaybeRef,
        ));
        helper.setup_bindings.push(SetupBinding(
            FervidAtom::from("Const"),
            BindingTypes::SetupConst,
        ));
        helper.setup_bindings.push(SetupBinding(
            FervidAtom::from("Let"),
            BindingTypes::SetupLet,
        ));
        helper.setup_bindings.push(SetupBinding(
            FervidAtom::from("Reactive"),
            BindingTypes::SetupReactiveConst,
        ));

        macro_rules! test {
            ($value: literal, $expected_value: literal, $expected_handler: literal) => {
                let mut v_model = VModelDirective {
                    argument: None,
                    value: js($value),
                    update_handler: None,
                    modifiers: vec![],
                    span: DUMMY_SP,
                };
                let mut patch_hints = PatchHints::default();
                helper.transform_v_model(&mut v_model, 0, &mut patch_hints);
                assert_eq!(to_str(&v_model.value), $expected_value);
                assert_eq!(
                    to_str(&v_model.update_handler.expect("Handler cannot be None")),
                    $expected_handler
                );
            };
        }

        // Syntax is like that:
        // first element is `$expr` in `v-model="$expr"`;
        // second is the transformed value;
        // third is the transformed update handler.

        // DEV DIRECT
        test!("Ref", "$setup.Ref", "$event=>$setup.Ref=$event");
        test!(
            "MaybeRef",
            "$setup.MaybeRef",
            "$event=>$setup.MaybeRef=$event"
        ); // TODO must err?
        test!("Const", "$setup.Const", "$event=>$setup.Const=$event"); // TODO must err
        test!("Let", "$setup.Let", "$event=>$setup.Let=$event");
        test!(
            "Reactive",
            "$setup.Reactive",
            "$event=>$setup.Reactive=$event"
        ); // TODO must err
        test!("Unknown", "_ctx.Unknown", "$event=>_ctx.Unknown=$event");

        // DEV INDIRECT
        test!("Ref.x", "$setup.Ref.x", "$event=>$setup.Ref.x=$event");
        test!(
            "MaybeRef.x",
            "$setup.MaybeRef.x",
            "$event=>$setup.MaybeRef.x=$event"
        );
        test!("Const.x", "$setup.Const.x", "$event=>$setup.Const.x=$event");
        test!("Let.x", "$setup.Let.x", "$event=>$setup.Let.x=$event");
        test!(
            "Reactive.x",
            "$setup.Reactive.x",
            "$event=>$setup.Reactive.x=$event"
        );
        test!(
            "Unknown.x",
            "_ctx.Unknown.x",
            "$event=>_ctx.Unknown.x=$event"
        );

        // PROD DIRECT
        helper.is_prod = true;
        helper.template_generation_mode = TemplateGenerationMode::Inline;
        test!("Ref", "Ref.value", "$event=>Ref.value=$event");
        test!(
            "MaybeRef",
            "_unref(MaybeRef)",
            "$event=>_isRef(MaybeRef)?MaybeRef.value=$event:null"
        );
        test!("Const", "Const", "$event=>Const=$event"); // TODO must err
        test!(
            "Let",
            "_unref(Let)",
            "$event=>_isRef(Let)?Let.value=$event:Let=$event"
        );
        test!("Reactive", "Reactive", "$event=>Reactive=$event"); // TODO must err
        test!("Unknown", "_ctx.Unknown", "$event=>_ctx.Unknown=$event");

        // PROD INDIRECT
        test!("Ref.x", "Ref.value.x", "$event=>Ref.value.x=$event");
        test!(
            "MaybeRef.x",
            "_unref(MaybeRef).x",
            "$event=>_unref(MaybeRef).x=$event"
        );
        test!("Const.x", "Const.x", "$event=>Const.x=$event");
        test!("Let.x", "_unref(Let).x", "$event=>_unref(Let).x=$event");
        test!("Reactive.x", "Reactive.x", "$event=>Reactive.x=$event");
        test!(
            "Unknown.x",
            "_ctx.Unknown.x",
            "$event=>_ctx.Unknown.x=$event"
        );
    }

    #[test]
    fn it_transforms_v_model_arg() {
        let mut helper = BindingsHelper::default();

        // const Ref = ref('foo')
        helper.setup_bindings.push(SetupBinding(
            FervidAtom::from("Ref"),
            BindingTypes::SetupRef,
        ));

        macro_rules! test {
            ($argument: literal, $expected: literal) => {
                let mut v_model = VModelDirective {
                    argument: Some(StrOrExpr::Expr(js($argument))),
                    value: js("dummy"),
                    update_handler: None,
                    modifiers: vec![],
                    span: DUMMY_SP,
                };
                let mut patch_hints = PatchHints::default();
                helper.transform_v_model(&mut v_model, 0, &mut patch_hints);
                let Some(StrOrExpr::Expr(arg_expr)) = v_model.argument else {
                    unreachable!("This is something unexpected")
                };
                assert_eq!(to_str(&arg_expr), $expected);
            };
        }

        // The first is `$expr` in `v-model:[$expr]="dummy"`, the second is expected

        // DEV
        test!("Ref", "$setup.Ref");
        test!("Unknown", "_ctx.Unknown");
        test!("\"string\"", "\"string\"");

        // PROD
        helper.is_prod = true;
        helper.template_generation_mode = TemplateGenerationMode::Inline;
        test!("Ref", "Ref.value");
        test!("Unknown", "_ctx.Unknown");
        test!("\"string\"", "\"string\"");
    }

    #[test]
    fn it_works_with_template_scope_hierarchy() {
        let v_root = FervidAtom::from("root");
        let root_scope = TemplateScope {
            parent: 0,
            variables: SmallVec::from(vec![v_root.to_owned()]),
        };

        let v_child1 = FervidAtom::from("child1");
        let v_child2 = FervidAtom::from("child2");
        let child_scope = TemplateScope {
            parent: 0,
            variables: SmallVec::from_vec(vec![v_child1.to_owned(), v_child2.to_owned()]),
        };

        let v_grand1_1 = FervidAtom::from("grand1_1");
        let v_grand1_2 = FervidAtom::from("grand1_2");
        let grandchild1_scope = TemplateScope {
            parent: 1,
            variables: SmallVec::from_vec(vec![v_grand1_1.to_owned(), v_grand1_2.to_owned()]),
        };

        let v_grand2_1 = FervidAtom::from("grand2_1");
        let v_grand2_2 = FervidAtom::from("grand2_2");
        let grandchild2_scope = TemplateScope {
            parent: 1,
            variables: SmallVec::from_vec(vec![v_grand2_1.to_owned(), v_grand2_2.to_owned()]),
        };

        let mut scope_helper = BindingsHelper::default();
        scope_helper.template_scopes.extend(vec![
            root_scope,
            child_scope,
            grandchild1_scope,
            grandchild2_scope,
        ]);

        // Measure time to get an idea on performance
        // TODO move this to Criterion
        let st0 = std::time::Instant::now();

        // All scopes have a root variable
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_root),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_root),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_root),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_root),
            BindingTypes::TemplateLocal
        );
        println!("Elapsed root: {:?}", st0.elapsed());

        // Only `child1` and its children have `child1` and `child2` vars
        let st1 = std::time::Instant::now();
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_child1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_child1),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_child1),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_child1),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_child2),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_child2),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_child2),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_child2),
            BindingTypes::TemplateLocal
        );
        println!("Elapsed child1: {:?}", st1.elapsed());

        // Only `grandchild1` has `grand1_1` and `grand1_2` vars
        let st2 = std::time::Instant::now();
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_grand1_1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_grand1_1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_grand1_1),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_grand1_1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_grand1_2),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_grand1_2),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_grand1_2),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_grand1_2),
            BindingTypes::Unresolved
        );
        println!("Elapsed grand1: {:?}", st2.elapsed());

        // Only `grandchild2` has `grand2_1` and `grand2_2` vars
        let st3 = std::time::Instant::now();
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_grand2_1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_grand2_1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_grand2_1),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_grand2_1),
            BindingTypes::TemplateLocal
        );
        assert_eq!(
            scope_helper.get_var_binding_type(0, &v_grand2_2),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(1, &v_grand2_2),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(2, &v_grand2_2),
            BindingTypes::Unresolved
        );
        assert_eq!(
            scope_helper.get_var_binding_type(3, &v_grand2_2),
            BindingTypes::TemplateLocal
        );
        println!("Elapsed grand2: {:?}", st3.elapsed());

        println!("Elapsed total: {:?}", st0.elapsed())
    }

    fn js(input: &str) -> Box<swc_core::ecma::ast::Expr> {
        parse_javascript_expr(input, 0, Default::default())
            .expect("js expects the input to be parseable")
            .0
    }
}
