use fervid_core::{
    fervid_atom, BindingTypes, FervidAtom, IntoIdent, PatchFlags, PatchHints, StrOrExpr,
    TemplateGenerationMode, VModelDirective, VueImports,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{
            ArrayLit, ArrayPat, AssignExpr, AssignOp, AssignTarget, AssignTargetPat, BindingIdent,
            BlockStmt, CallExpr, Callee, CondExpr, Decl, Expr, ExprOrSpread, Ident, IdentName,
            KeyValuePatProp, KeyValueProp, Lit, MemberExpr, MemberProp, Null, ObjectLit, ObjectPat,
            ObjectPatProp, Pat, Prop, PropName, PropOrSpread, SimpleAssignTarget, Stmt, UpdateExpr,
            UpdateOp,
        },
        visit::{VisitMut, VisitMutWith},
    },
};

use crate::{
    script::common::extract_variables_from_pat, template::js_builtins::JS_BUILTINS, BindingsHelper,
    SetupBinding,
};

use super::utils::wrap_in_event_arrow;

struct TransformVisitor<'s> {
    current_scope: u32,
    bindings_helper: &'s mut BindingsHelper,
    has_js_bindings: bool,
    is_inline: bool,

    /// In ({ x } = y)
    is_in_destructure_assign: bool,

    /// In LHS of x = y
    is_in_assign_target: bool,

    /// COMPAT: `v-on` and `v-model` look differently at how to transform `SetupMaybeRef`
    /// `v-on` simply assigns to it: `maybe.value = 1`
    /// `v-model` does a check: `isRef(maybe) ? maybe.value = 1 : null`
    is_v_model_transform: bool,

    // `SetupBinding` instead of `FervidAtom` to easier interface with `extract_variables_from_pat`
    local_vars: Vec<SetupBinding>,

    // https://github.com/vuejs/core/blob/9e8ac0c367522922b5d8442b5a3cc508666978af/packages/compiler-core/src/transforms/transformExpression.ts#L126-L135
    // For transforming `x = y` where LHS is an identifier
    update_expr_helper: Option<(UpdateOp, bool)>,
    should_consume_update_expr: bool,
}

#[derive(Debug)]
pub enum IdentTransformStrategy {
    /// Leave the identifier as-is, e.g. for global symbols or template-local variables coming from `v-for`
    LeaveUnchanged,
    /// Append the `.value`
    DotValue,
    /// Wrap in `unref()`
    Unref,
    /// Add the prefix, e.g. `$setup` or `_ctx`
    Prefix(FervidAtom),
    /// Generate `isRef(e) ? e.value++ : e++`
    IsRefCheckUpdate,
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
            is_in_assign_target: false,
            is_in_destructure_assign: false,
            is_v_model_transform: false,
            local_vars: Vec::new(),
            update_expr_helper: None,
            should_consume_update_expr: false,
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
        // 0. Ensure that `v-model` value is a valid AssignTarget
        let Some(assign_target) = convert_expr_to_assign_target(v_model.value.to_owned()) else {
            // TODO Error
            return;
        };

        // 1. Create handler: wrap in `$event => value = $event`
        let event_expr = Box::new(Expr::Ident(FervidAtom::from("$event").into_ident()));
        let mut handler = wrap_in_event_arrow(wrap_in_assignment(
            assign_target,
            event_expr,
            AssignOp::Assign,
        ));

        // 2. Transform handler
        {
            let is_inline = matches!(
                self.template_generation_mode,
                TemplateGenerationMode::Inline
            );
            let mut visitor = TransformVisitor {
                current_scope: scope_to_use,
                bindings_helper: self,
                has_js_bindings: false,
                is_inline,
                is_in_assign_target: false,
                is_in_destructure_assign: false,
                is_v_model_transform: true,
                local_vars: Vec::new(),
                update_expr_helper: None,
                should_consume_update_expr: false,
            };
            handler.visit_mut_with(&mut visitor);
        }

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
        // DOCTEXT: Disallow `SetupConst` or `SetupReactiveConst` to be used as a `v-model` value or as an assignment target.
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
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        let ident: &mut Ident = match expr {
            // Special treatment for assignment expression
            Expr::Assign(assign_expr) => {
                // Visit RHS first
                assign_expr.right.visit_mut_with(self);

                // Check for special case: LHS is an ident of type `SetupLet`
                // Also special case: LHS is `SetupMaybeRef` inside v-model
                // This is only valid for Inline mode
                let setup_let_ident = if self.is_inline {
                    let ident = match assign_expr.left {
                        AssignTarget::Simple(SimpleAssignTarget::Ident(ref id)) => Some(&id.id),
                        _ => None,
                    };

                    ident.and_then(|ident| {
                        let binding_type = self
                            .bindings_helper
                            .get_var_binding_type(self.current_scope, &ident.sym);

                        if matches!(binding_type, BindingTypes::SetupLet)
                            || self.is_v_model_transform
                                && matches!(binding_type, BindingTypes::SetupMaybeRef)
                        {
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

                    *expr = *generate_is_ref_check_assignment(
                        ident,
                        &assign_expr.right,
                        assign_expr.op,
                        self.bindings_helper,
                        is_reassignable,
                    );
                } else {
                    // Process as usual
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

            // Regular functions need params collection as well
            Expr::Fn(fn_expr) => {
                let old_len = self.local_vars.len();
                let func = &mut fn_expr.function;

                // Add the temporary variables
                self.local_vars.reserve(func.params.len());
                for param in func.params.iter() {
                    extract_variables_from_pat(&param.pat, &mut self.local_vars, true);
                }

                // Transform the function body
                func.body.visit_mut_with(self);

                // Clear the temporary variables
                self.local_vars.drain(old_len..);

                return;
            }

            // Update expression, `SetupLet` is a special case here
            Expr::Update(update_expr) => {
                if let Expr::Ident(_) = *update_expr.arg {
                    self.update_expr_helper = Some((update_expr.op, update_expr.prefix));
                    update_expr.arg.visit_mut_with(self);
                    self.update_expr_helper = None;

                    // AGREEMENT: If `should_consume_update_expr` is set,
                    // assign transformed expr instead (this handles `lett++` case)
                    if self.should_consume_update_expr {
                        self.should_consume_update_expr = false;
                        *expr = *update_expr.arg.to_owned();
                    }
                } else {
                    expr.visit_mut_children_with(self);
                }
                return;
            }
            
            // Call expression, type arguments should not be taken into account
            Expr::Call(call_expr) => {
                call_expr.callee.visit_mut_with(self);
                for arg in call_expr.args.iter_mut() {
                    arg.visit_mut_with(self);
                }
                return;
            }

            // Identifier is what we need for the rest of the function
            Expr::Ident(ident_expr) => ident_expr,

            _ => {
                expr.visit_mut_children_with(self);
                return;
            }
        };

        // The rest concerns transforming an ident
        let span = ident.span;
        let strategy = self.determine_ident_transform_strategy(ident);

        // TODO The logic for setup variables actually differs quite significantly
        // https://play.vuejs.org/#eNp9UU1rwzAM/SvCl25QEkZvIRTa0cN22Mq6oy8hUVJ3iW380QWC//tkh2Y7jN6k956kJ2liO62zq0dWsNLWRmgHFp3XWy7FoJVxMMEOArRGDbDK8v2KywZbIfFolLYPE5cArVIFnJwRsuMyPHJZ5nMv6kKJw0H3lUPKAMrz03aaYgmEQAE1D2VOYKxalGzNnK2VbEWXXaySZC9N4qxWgxY9mnfthJKWswISE7mq79X3a8Kc8bi+4fUZ669/8IsdI8bZ0aBFc0XOFs5VpkM304fTG44UL+SgGt+T+g75gVb1PnqcZXsvG7L9R5fcvqQj0+E+7WF0KO1tqWg0KkPSc0Y/er6z+q/dTbZJdfQJFn4A+DKelw==

        match strategy {
            IdentTransformStrategy::LeaveUnchanged => return,

            IdentTransformStrategy::DotValue => {
                *expr = Expr::Member(MemberExpr {
                    span,
                    obj: Box::new(expr.to_owned()),
                    prop: MemberProp::Ident(fervid_atom!("value").into_ident().into()),
                });
                return;
            }

            IdentTransformStrategy::Unref => {
                self.bindings_helper.vue_imports |= VueImports::Unref;

                *expr = Expr::Call(CallExpr {
                    span,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(
                        VueImports::Unref.as_atom().into_ident_spanned(span),
                    ))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::new(expr.to_owned()),
                    }],
                    type_args: None,
                });
                return;
            }

            IdentTransformStrategy::Prefix(prefix) => {
                *expr = Expr::Member(MemberExpr {
                    span,
                    obj: Box::new(Expr::Ident(prefix.into_ident_spanned(span))),
                    prop: MemberProp::Ident(ident.to_owned().into()),
                });
                return;
            }

            IdentTransformStrategy::IsRefCheckUpdate => {
                let Some((update_op, update_prefix)) = self.update_expr_helper.take() else {
                    // TODO This should be unreachable, signify error
                    return;
                };

                // Signify that this is a rewrite
                self.should_consume_update_expr = true;
                *expr = generate_is_ref_check_update(
                    ident,
                    update_op,
                    update_prefix,
                    self.bindings_helper,
                );
                return;
            }
        }
    }

    fn visit_mut_member_expr(&mut self, n: &mut MemberExpr) {
        if n.obj.is_ident() {
            n.obj.visit_mut_with(self)
        } else {
            n.visit_mut_children_with(self);
        }
    }

    fn visit_mut_object_lit(&mut self, n: &mut ObjectLit) {
        for prop in n.props.iter_mut() {
            match prop {
                PropOrSpread::Prop(ref mut prop) => {
                    // For shorthand, expand it and visit the value part
                    if let Some(shorthand) = prop.as_mut_shorthand() {
                        let prop_name = PropName::Ident(IdentName {
                            span: shorthand.span,
                            sym: shorthand.sym.to_owned(),
                        });

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

    // Visit the block as with respect to the variables
    fn visit_mut_block_stmt(&mut self, block_stmt: &mut BlockStmt) {
        // All variables will be treated as block scope
        let old_len = self.local_vars.len();

        for stmt in block_stmt.stmts.iter_mut() {
            // Add the temporary variables
            if let Stmt::Decl(decl) = stmt {
                match decl {
                    Decl::Class(cls) => self.local_vars.push(SetupBinding(
                        cls.ident.sym.to_owned(),
                        BindingTypes::TemplateLocal,
                    )),
                    Decl::Fn(fn_decl) => self.local_vars.push(SetupBinding(
                        fn_decl.ident.sym.to_owned(),
                        BindingTypes::TemplateLocal,
                    )),
                    Decl::Var(var_decl) => {
                        for var_decl_it in var_decl.decls.iter() {
                            extract_variables_from_pat(
                                &var_decl_it.name,
                                &mut self.local_vars,
                                true,
                            );
                        }
                    }
                    _ => {}
                }
            }

            stmt.visit_mut_with(self);
        }

        // Clear the temporary variables
        self.local_vars.drain(old_len..);
    }

    // This is a copy of `visit_mut_expr` because AssignTarget is more refined compared to Expr
    fn visit_mut_assign_target(&mut self, n: &mut AssignTarget) {
        let old_is_in_assign_target = self.is_in_assign_target;
        self.is_in_assign_target = true;

        match n {
            AssignTarget::Simple(simple) => match simple {
                SimpleAssignTarget::Ident(ident) => {
                    let strategy = self.determine_ident_transform_strategy(&ident.id);
                    let span = ident.span;

                    match strategy {
                        IdentTransformStrategy::LeaveUnchanged => return,

                        IdentTransformStrategy::DotValue => {
                            *n = AssignTarget::Simple(SimpleAssignTarget::Member(MemberExpr {
                                span,
                                obj: Box::new(Expr::Ident(ident.id.to_owned())),
                                prop: MemberProp::Ident(IdentName {
                                    span: DUMMY_SP,
                                    sym: fervid_atom!("value"),
                                }),
                            }));
                            return;
                        }

                        IdentTransformStrategy::Prefix(prefix) => {
                            *n = AssignTarget::Simple(SimpleAssignTarget::Member(MemberExpr {
                                span,
                                obj: Box::new(Expr::Ident(prefix.into_ident())),
                                prop: MemberProp::Ident(ident.id.to_owned().into()),
                            }));
                            return;
                        }

                        IdentTransformStrategy::Unref
                        | IdentTransformStrategy::IsRefCheckUpdate => {
                            // TODO Error: this is not a valid transform strategy
                            // Error hint: this is a bug in `fervid`, please report it
                        }
                    }
                }

                SimpleAssignTarget::Member(member) => member.visit_mut_with(self),
                SimpleAssignTarget::SuperProp(sup) => sup.visit_mut_with(self),
                SimpleAssignTarget::Paren(paren) => paren.visit_mut_with(self),
                SimpleAssignTarget::OptChain(opt_chain) => opt_chain.visit_mut_with(self),
                SimpleAssignTarget::TsAs(ts_as) => ts_as.visit_mut_with(self),
                SimpleAssignTarget::TsSatisfies(sat) => sat.visit_mut_with(self),
                SimpleAssignTarget::TsNonNull(non_null) => non_null.visit_mut_with(self),
                SimpleAssignTarget::TsTypeAssertion(type_assert) => {
                    type_assert.visit_mut_with(self)
                }
                SimpleAssignTarget::TsInstantiation(inst) => inst.visit_mut_with(self),
                SimpleAssignTarget::Invalid(_) => {}
            },

            AssignTarget::Pat(assign_target_pat) => {
                let old_is_in_destructure = self.is_in_destructure_assign;
                self.is_in_destructure_assign = true;

                match assign_target_pat {
                    AssignTargetPat::Array(arr_pat) => arr_pat.visit_mut_with(self),
                    AssignTargetPat::Object(obj_pat) => obj_pat.visit_mut_with(self),
                    AssignTargetPat::Invalid(_) => {}
                };

                self.is_in_destructure_assign = old_is_in_destructure;
            }
        }

        self.is_in_assign_target = old_is_in_assign_target;
    }

    fn visit_mut_pat(&mut self, n: &mut Pat) {
        if !self.is_in_destructure_assign {
            n.visit_mut_children_with(self);
            return;
        };

        match n {
            Pat::Ident(ident) => {
                let strategy = self.determine_ident_transform_strategy(&ident.id);
                let span = ident.span;

                match strategy {
                    IdentTransformStrategy::LeaveUnchanged => return,

                    IdentTransformStrategy::DotValue => {
                        *n = Pat::Expr(Box::new(Expr::Member(MemberExpr {
                            span,
                            obj: Box::new(Expr::Ident(ident.id.to_owned())),
                            prop: MemberProp::Ident(IdentName {
                                span: DUMMY_SP,
                                sym: fervid_atom!("value"),
                            }),
                        })));
                        return;
                    }

                    IdentTransformStrategy::Prefix(prefix) => {
                        *n = Pat::Expr(Box::new(Expr::Member(MemberExpr {
                            span: DUMMY_SP,
                            obj: Box::new(Expr::Ident(prefix.into_ident())),
                            prop: MemberProp::Ident(ident.id.to_owned().into()),
                        })));
                        return;
                    }

                    IdentTransformStrategy::Unref | IdentTransformStrategy::IsRefCheckUpdate => {
                        // TODO Error: this is not a valid transform strategy
                        // (technically this is a syntax error, so should be impossible)
                    }
                }
            }

            Pat::Array(arr_pat) => arr_pat.visit_mut_with(self),
            Pat::Rest(rest_pat) => rest_pat.arg.visit_mut_with(self),
            Pat::Object(obj_pat) => obj_pat.visit_mut_with(self),
            Pat::Assign(assign_pat) => assign_pat.visit_mut_with(self),
            Pat::Expr(expr) => expr.visit_mut_with(self),
            Pat::Invalid(_) => {}
        }
    }

    fn visit_mut_array_pat(&mut self, arr_pat: &mut ArrayPat) {
        for maybe_pat in arr_pat.elems.iter_mut() {
            let Some(pat) = maybe_pat else {
                continue;
            };

            pat.visit_mut_with(self)
        }
    }

    fn visit_mut_object_pat(&mut self, obj_pat: &mut ObjectPat) {
        for elem in obj_pat.props.iter_mut() {
            match elem {
                // `{ x: y }`
                ObjectPatProp::KeyValue(key_value) => {
                    key_value.value.visit_mut_with(self);

                    match key_value.key {
                        // TODO Finish the implementation
                        PropName::Ident(_) => todo!(),
                        PropName::Computed(_) => todo!(),
                        PropName::Str(_) | PropName::Num(_) | PropName::BigInt(_) => {}
                    }
                }

                ObjectPatProp::Assign(assign) => {
                    match assign.value {
                        // `{ x = y }`
                        Some(ref mut v) => v.visit_mut_with(self),

                        // If shorthand `{ x }`, expand when not a local variable
                        None => {
                            let symbol = &assign.key.sym;

                            let is_local =
                                self.local_vars.iter().rfind(|it| &it.0 == symbol).is_some()
                                    || matches!(
                                        self.bindings_helper
                                            .get_var_binding_type(self.current_scope, symbol),
                                        BindingTypes::TemplateLocal
                                    );

                            if !is_local {
                                let mut value = Box::new(Pat::Ident(assign.key.to_owned()));
                                value.visit_mut_with(self);
                                *elem = ObjectPatProp::KeyValue(KeyValuePatProp {
                                    key: PropName::Ident(assign.key.id.to_owned().into()),
                                    value,
                                })
                            }
                        }
                    }
                }

                // The official compiler seems to ignore this one
                ObjectPatProp::Rest(_) => {}
            }
        }
    }
}

impl TransformVisitor<'_> {
    /// Determines the strategy with which an Ident needs to be transformed.
    /// This function is needed because SWC's AST is strongly-typed and we cannot simply
    /// transform the Ident as the official compiler does.
    pub fn determine_ident_transform_strategy(&mut self, ident: &Ident) -> IdentTransformStrategy {
        let symbol = &ident.sym;

        // Try to find variable in the local vars (e.g. arrow function params)
        if let Some(_) = self.local_vars.iter().rfind(|it| &it.0 == symbol) {
            self.has_js_bindings = true;
            return IdentTransformStrategy::LeaveUnchanged;
        }

        let binding_type = self
            .bindings_helper
            .get_var_binding_type(self.current_scope, symbol);

        // Template local binding doesn't need any processing
        if let BindingTypes::TemplateLocal = binding_type {
            self.has_js_bindings = true;
            return IdentTransformStrategy::LeaveUnchanged;
        }

        // Get the prefix which fits the scope (e.g. `_ctx.` for unknown scopes, `$setup.` for setup scope)
        if let Some(prefix) = get_prefix(&binding_type, self.is_inline) {
            self.has_js_bindings = true;
            return IdentTransformStrategy::Prefix(prefix);
        }

        // Non-inline logic ends here
        if !self.is_inline {
            return IdentTransformStrategy::LeaveUnchanged;
        }

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

        match binding_type {
            // Update expression with MaybeRef: `maybe++` -> `maybe.value++`
            BindingTypes::SetupMaybeRef
                if self.is_in_destructure_assign
                    || (self.is_in_assign_target && !self.is_v_model_transform)
                    || self.update_expr_helper.is_some() =>
            {
                IdentTransformStrategy::DotValue
            }

            // Update expression with SetupLet: `lett++` -> `isRef(lett) ? lett.value++ : lett++`
            BindingTypes::SetupLet if self.update_expr_helper.is_some() => {
                IdentTransformStrategy::IsRefCheckUpdate
            }

            BindingTypes::SetupMaybeRef | BindingTypes::SetupLet | BindingTypes::Imported => {
                IdentTransformStrategy::Unref
            }
            BindingTypes::SetupRef => IdentTransformStrategy::DotValue,
            BindingTypes::SetupConst
            | BindingTypes::SetupReactiveConst
            | BindingTypes::LiteralConst => IdentTransformStrategy::LeaveUnchanged,
            _ => IdentTransformStrategy::LeaveUnchanged,
        }
    }
}

/// Gets the variable prefix depending on if we are compiling the template in inline mode.
/// This is used for transformations.
/// ## Example
/// `data()` variable `foo` in non-inline compilation becomes `$data.foo`.\
/// `setup()` ref variable `bar` in non-inline compilation becomes `$setup.bar`,
/// but in the inline compilation it remains the same.
pub fn get_prefix(binding_type: &BindingTypes, is_inline: bool) -> Option<FervidAtom> {
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
        BindingTypes::TemplateLocal
        | BindingTypes::JsGlobal
        | BindingTypes::LiteralConst
        | BindingTypes::Component
        | BindingTypes::Imported => None,
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
    op: AssignOp,
    bindings_helper: &mut BindingsHelper,
    is_reassignable: bool,
) -> Box<Expr> {
    // Get `isRef` helper
    let is_ref_ident = VueImports::IsRef.as_atom();
    bindings_helper.vue_imports |= VueImports::IsRef;

    // `ident` expression
    let ident_expr = Box::new(Expr::Ident(lhs_ident.to_owned()));
    let ident_assign_target = AssignTarget::Simple(SimpleAssignTarget::Ident(BindingIdent {
        id: lhs_ident.to_owned(),
        type_ann: None,
    }));

    // `isRef(ident)`
    let condition = Box::new(Expr::Call(CallExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(is_ref_ident.into_ident()))),
        args: vec![ExprOrSpread {
            spread: None,
            expr: ident_expr.to_owned(),
        }],
        type_args: None,
    }));

    // `ident.value`
    let ident_dot_value = AssignTarget::Simple(SimpleAssignTarget::Member(MemberExpr {
        span: DUMMY_SP,
        obj: ident_expr.to_owned(),
        prop: MemberProp::Ident(IdentName {
            span: DUMMY_SP,
            sym: FervidAtom::from("value"),
        }),
    }));

    // `ident.value = rhs_expr`
    let positive_assign = wrap_in_assignment(ident_dot_value, Box::new(rhs_expr.to_owned()), op);

    // `ident = rhs_expr` or `null`
    let negative_assign = if is_reassignable {
        wrap_in_assignment(ident_assign_target, Box::new(rhs_expr.to_owned()), op)
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

/// Generates `_isRef(ident) ? ident.value++ : ident++`
fn generate_is_ref_check_update(
    ident: &Ident,
    op: UpdateOp,
    prefix: bool,
    bindings_helper: &mut BindingsHelper,
) -> Expr {
    // Get `isRef` helper
    let is_ref_ident = VueImports::IsRef.as_atom();
    bindings_helper.vue_imports |= VueImports::IsRef;

    // `ident` expression
    let ident_expr = Box::new(Expr::Ident(ident.to_owned()));

    // `isRef(ident)`
    let condition = Box::new(Expr::Call(CallExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(is_ref_ident.into_ident()))),
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
        prop: MemberProp::Ident(IdentName {
            span: DUMMY_SP,
            sym: FervidAtom::from("value"),
        }),
    }));

    // `ident.value++`
    let positive_update = Box::new(Expr::Update(UpdateExpr {
        span: DUMMY_SP,
        op,
        prefix,
        arg: ident_dot_value,
    }));

    // `ident++`
    let negative_update = Box::new(Expr::Update(UpdateExpr {
        span: DUMMY_SP,
        op,
        prefix,
        arg: ident_expr,
    }));

    Expr::Cond(CondExpr {
        span: DUMMY_SP,
        test: condition,
        cons: positive_update,
        alt: negative_update,
    })
}

/// Wraps `expr` to `expr = $event`
#[inline]
fn wrap_in_assignment(lhs: AssignTarget, rhs_expr: Box<Expr>, op: AssignOp) -> Box<Expr> {
    Box::new(Expr::Assign(AssignExpr {
        span: DUMMY_SP,
        op,
        left: lhs,
        right: rhs_expr,
    }))
}

fn convert_expr_to_assign_target(expr: Box<Expr>) -> Option<AssignTarget> {
    // Because AssignTarget is strongly typed, we have to map from `Expr` to `AssignTarget`
    match *expr {
        Expr::Array(arr) => Some(AssignTarget::Pat(AssignTargetPat::Array(
            convert_arr_lit_to_pat(arr),
        ))),

        Expr::Object(obj) => Some(AssignTarget::Pat(AssignTargetPat::Object(
            convert_obj_lit_to_pat(obj),
        ))),

        Expr::Ident(ident) => Some(AssignTarget::Simple(SimpleAssignTarget::Ident(
            BindingIdent {
                id: ident,
                type_ann: None, // required by SWC
            },
        ))),

        Expr::Member(member) => Some(AssignTarget::Simple(SimpleAssignTarget::Member(member))),
        Expr::Paren(paren) => Some(AssignTarget::Simple(SimpleAssignTarget::Paren(paren))),
        Expr::SuperProp(super_prop) => Some(AssignTarget::Simple(SimpleAssignTarget::SuperProp(
            super_prop,
        ))),
        Expr::OptChain(opt_chain) => Some(AssignTarget::Simple(SimpleAssignTarget::OptChain(
            opt_chain,
        ))),
        Expr::TsNonNull(non_null) => Some(AssignTarget::Simple(SimpleAssignTarget::TsNonNull(
            non_null,
        ))),
        Expr::TsAs(ts_as) => Some(AssignTarget::Simple(SimpleAssignTarget::TsAs(ts_as))),
        Expr::TsInstantiation(inst) => Some(AssignTarget::Simple(
            SimpleAssignTarget::TsInstantiation(inst),
        )),
        Expr::TsSatisfies(sat) => Some(AssignTarget::Simple(SimpleAssignTarget::TsSatisfies(sat))),
        Expr::TsTypeAssertion(type_assert) => Some(AssignTarget::Simple(
            SimpleAssignTarget::TsTypeAssertion(type_assert),
        )),

        // Maybe some other expressions can be a valid assignment target, but I trust SWC here
        _ => None,
    }
}

fn convert_arr_lit_to_pat(_arr_lit: ArrayLit) -> ArrayPat {
    todo!()
}

fn convert_obj_lit_to_pat(_obj_lit: ObjectLit) -> ObjectPat {
    todo!()
}

#[cfg(test)]
mod tests {
    use crate::{
        template::{expr_transform::BindingsHelperTransform, js_builtins::JS_BUILTINS},
        test_utils::{parser::parse_javascript_expr, to_str},
        BindingsHelper, SetupBinding, TemplateScope,
    };
    use fervid_core::{
        BindingTypes, FervidAtom, PatchHints, StrOrExpr, TemplateGenerationMode, VModelDirective,
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
            // This is the official spec, but it is inconsistent with the `v-on` transform
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
