use fervid_core::{BindingTypes, FervidAtom, TemplateGenerationMode};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::{
        ast::{
            CallExpr, Expr, Ident, KeyValueProp, MemberExpr, MemberProp, Prop, PropName,
            PropOrSpread, Callee, ExprOrSpread, PatOrExpr,
        },
        atoms::JsWord,
        visit::{VisitMut, VisitMutWith},
    },
};

use crate::{structs::ScopeHelper, template::js_builtins::JS_BUILTINS};

struct TransformVisitor<'s> {
    current_scope: u32,
    scope_helper: &'s mut ScopeHelper,
    has_js_bindings: bool,
    is_inline: bool,
    is_write: bool
}

impl ScopeHelper {
    // TODO This function needs to be invoked when an AST is being optimized
    // TODO Support transformation modes (e.g. `inline`, `renderFn`)
    pub fn transform_expr(&mut self, expr: &mut Expr, scope_to_use: u32) -> bool {
        let is_inline = matches!(self.template_generation_mode, TemplateGenerationMode::Inline);
        let mut visitor = TransformVisitor {
            current_scope: scope_to_use,
            scope_helper: self,
            has_js_bindings: false,
            is_inline,
            is_write: false
        };
        expr.visit_mut_with(&mut visitor);

        visitor.has_js_bindings
    }

    pub fn get_var_binding_type(&mut self, starting_scope: u32, variable: &str) -> BindingTypes {
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
        let variable_atom = FervidAtom::from(variable);
        if let Some(binding_type) = self.used_idents.get(&variable_atom) {
            return binding_type.to_owned();
        }

        // Check setup bindings (both `<script setup>` and `setup()`)
        let setup_bindings = self.setup_bindings.iter().chain(
            self.options_api_vars
                .as_ref()
                .map_or_else(|| [].iter(), |v| v.setup.iter()),
        );
        for binding in setup_bindings {
            if binding.0 == variable_atom {
                self.used_idents.insert(variable_atom, binding.1);
                return binding.1;
            }
        }

        // Macro to check if the variable is in the slice/Vec and conditionally return
        macro_rules! check_scope {
            ($vars: expr, $ret_descriptor: expr) => {
                if $vars.iter().any(|it| *it == variable_atom) {
                    self.used_idents.insert(variable_atom, $ret_descriptor);
                    return $ret_descriptor;
                }
            };
        }

        // Check all the options API variables
        if let Some(options_api_vars) = &self.options_api_vars {
            check_scope!(options_api_vars.data, BindingTypes::Data);
            check_scope!(options_api_vars.props, BindingTypes::Props);
            check_scope!(options_api_vars.computed, BindingTypes::Options);
            check_scope!(options_api_vars.methods, BindingTypes::Options);
            check_scope!(options_api_vars.inject, BindingTypes::Options);

            // Check options API imports.
            // Currently it ignores the SyntaxContext (same as in js implementation)
            for binding in options_api_vars.imports.iter() {
                if binding.0 == variable_atom {
                    self.used_idents
                        .insert(variable_atom, BindingTypes::SetupMaybeRef);
                    return BindingTypes::SetupMaybeRef;
                }
            }
        }

        BindingTypes::Unresolved
    }
}

impl<'s> VisitMut for TransformVisitor<'s> {
    fn visit_mut_assign_expr(&mut self, n: &mut swc_core::ecma::ast::AssignExpr) {
        match n.left {
            // Assignments must have their LHS correctly handled
            // Especially `SetupLet`
            PatOrExpr::Expr(ref e) if matches!(**e, Expr::Ident(_)) => {
                let old_is_write = self.is_write;
                self.is_write = true;
                n.left.visit_mut_with(self);
                self.is_write = old_is_write;
                n.right.visit_mut_with(self);
            }

            _ => {
                n.visit_mut_children_with(self)
            }
        }        
    }

    fn visit_mut_expr(&mut self, n: &mut Expr) {
        let Expr::Ident(ident_expr) = n else {
            n.visit_mut_children_with(self);
            return;
        };

        let symbol = &ident_expr.sym;
        let span = ident_expr.span;

        let binding_type = self
            .scope_helper
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

        let unref = |expr: &mut Expr, span: Span| {
            // TODO Import `_unref` somehow
            // TODO Rename `ScopeHelper` to `BindingsHelper` and add `vue_imports` there
            *expr = Expr::Call(CallExpr {
                span,
                callee: Callee::Expr(Box::new(Expr::Ident(Ident { span, sym: JsWord::from("_unref"), optional: false }))),
                args: vec![
                    ExprOrSpread { spread: None, expr: Box::new(expr.to_owned()) }
                ],
                type_args: None,
            });
        };

        // Inline logic is pretty complex
        // TODO Actual logic
        match binding_type {
            BindingTypes::SetupLet => unref(n, span),
            BindingTypes::SetupConst => {},
            BindingTypes::SetupReactiveConst => {},
            BindingTypes::SetupMaybeRef => unref(n, span),
            BindingTypes::SetupRef => dot_value(n, span),
            BindingTypes::LiteralConst => {},
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
            BindingTypes::Data | BindingTypes::Options | BindingTypes::Unresolved => Some(JsWord::from("_ctx")),
            BindingTypes::Props => Some(JsWord::from("__props")),
            // TODO This is not correct. The transform implementation must handle `unref`
            _ => None,
        };
    }

    match binding_type {
        BindingTypes::Data => Some(JsWord::from("$data")),
        BindingTypes::Props => Some(JsWord::from("$props")),
        BindingTypes::Options => Some(JsWord::from("$options")),
        BindingTypes::TemplateLocal | BindingTypes::JsGlobal | BindingTypes::LiteralConst => None,
        BindingTypes::SetupConst
        | BindingTypes::SetupLet
        | BindingTypes::SetupMaybeRef
        | BindingTypes::SetupReactiveConst
        | BindingTypes::SetupRef => Some(JsWord::from("$setup")),
        BindingTypes::Unresolved => Some(JsWord::from("_ctx")),
        BindingTypes::PropsAliased => unimplemented!(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        structs::{ScopeHelper, TemplateScope},
        template::js_builtins::JS_BUILTINS,
    };
    use fervid_core::{BindingTypes, TemplateGenerationMode};
    use smallvec::SmallVec;
    use swc_core::ecma::atoms::JsWord;

    #[test]
    fn it_acknowledges_builtins() {
        let mut helper = ScopeHelper::default();

        for builtin in JS_BUILTINS.iter() {
            assert_eq!(
                BindingTypes::JsGlobal,
                helper.get_var_binding_type(0, builtin)
            );
        }

        // Check inline mode as well
        helper.template_generation_mode = TemplateGenerationMode::Inline;
        for builtin in JS_BUILTINS.iter() {
            assert_eq!(
                BindingTypes::JsGlobal,
                helper.get_var_binding_type(0, builtin)
            );
        }
    }

    #[test]
    fn it_works_with_template_scope_hierarchy() {
        let v_root = JsWord::from("root");
        let root_scope = TemplateScope {
            parent: 0,
            variables: SmallVec::from([v_root.to_owned()]),
        };

        let v_child1 = JsWord::from("child1");
        let v_child2 = JsWord::from("child2");
        let child_scope = TemplateScope {
            parent: 0,
            variables: SmallVec::from_vec(vec![v_child1.to_owned(), v_child2.to_owned()]),
        };

        let v_grand1_1 = JsWord::from("grand1_1");
        let v_grand1_2 = JsWord::from("grand1_2");
        let grandchild1_scope = TemplateScope {
            parent: 1,
            variables: SmallVec::from_vec(vec![v_grand1_1.to_owned(), v_grand1_2.to_owned()]),
        };

        let v_grand2_1 = JsWord::from("grand2_1");
        let v_grand2_2 = JsWord::from("grand2_2");
        let grandchild2_scope = TemplateScope {
            parent: 1,
            variables: SmallVec::from_vec(vec![v_grand2_1.to_owned(), v_grand2_2.to_owned()]),
        };

        let mut scope_helper = ScopeHelper::default();
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
}
