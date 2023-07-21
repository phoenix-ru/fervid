use fervid_core::VueDirectives;
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::{
        ast::{
            ArrayLit, BindingIdent, Bool, CallExpr, Callee, Expr, ExprOrSpread, Ident,
            KeyValueProp, Lit, Number, ObjectLit, Pat, Prop, PropOrSpread, Str, UnaryExpr,
            UnaryOp, VarDeclarator,
        },
        atoms::JsWord,
    },
};

use crate::{imports::VueImports, utils::str_to_propname, CodegenContext};

mod v_model;

impl CodegenContext {
    pub fn generate_directives_to_array(
        &mut self,
        directives: &VueDirectives,
        out: &mut Vec<Option<ExprOrSpread>>,
    ) {
        // Check for work and possibly pre-allocate
        macro_rules! has {
            ($key: ident) => {
                directives.$key.is_some() as usize
            };
        }
        let total_work = directives.custom.len() + has!(v_show) + has!(v_memo);
        if total_work == 0 {
            return;
        }

        // Pre-allocate
        out.reserve(total_work);

        // v-show
        if let Some(ref v_show) = directives.v_show {
            let span = DUMMY_SP; // TODO Span
            let v_show_ident = Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::VShow),
                optional: false,
            };

            out.push(Some(ExprOrSpread {
                spread: None,
                expr: Box::new(self.generate_directive_from_parts(
                    v_show_ident,
                    Some(v_show),
                    None,
                    &[],
                    span,
                )),
            }))
        }

        // Generate custom directives last
        for custom_directive in directives.custom.iter() {
            let span = DUMMY_SP; // TODO Span
            let directive_ident = self.get_custom_directive_ident(custom_directive.name, span);

            out.push(Some(ExprOrSpread {
                spread: None,
                expr: Box::new(self.generate_directive_from_parts(
                    directive_ident,
                    custom_directive.value.as_deref(),
                    custom_directive.argument,
                    &custom_directive.modifiers,
                    span,
                )),
            }));
        }
    }

    /// Generates `withDirectives(/* render code */, [/* directives array */])`
    pub fn maybe_generate_with_directives(
        &mut self,
        expr: Expr,
        directives_arr: Vec<Option<ExprOrSpread>>,
    ) -> Expr {
        if directives_arr.len() == 0 {
            return expr;
        }

        Expr::Call(CallExpr {
            span: DUMMY_SP, // TODO Span
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: self.get_and_add_import_ident(VueImports::WithDirectives),
                optional: false,
            }))),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(expr),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Array(ArrayLit {
                        span: DUMMY_SP,
                        elems: directives_arr,
                    })),
                },
            ],
            type_args: None,
        })
    }

    /// Generates a generalized directive in form
    /// `[
    ///   directive_ident,
    ///   directive_expression?,
    ///   directive_arg?,
    ///   { modifier1: true, modifier2: true }?
    /// ]`.
    ///
    /// This typically applies to custom directives, `v-show` and element `v-model`
    pub fn generate_directive_from_parts(
        &mut self,
        name: Ident,
        value: Option<&Expr>,
        argument: Option<&str>,
        modifiers: &[&str],
        span: Span,
    ) -> Expr {
        let has_argument = argument.is_some();
        let has_modifiers = modifiers.len() > 0;

        // Array and size hint
        let directive_arr_len_hint = if has_modifiers {
            4
        } else if has_argument {
            3
        } else if value.is_some() {
            2
        } else {
            1
        };
        let mut directive_arr = ArrayLit {
            span,
            elems: Vec::with_capacity(directive_arr_len_hint),
        };

        // Directive name
        // let directive_ident = self.get_custom_directive_ident(custom_directive.name, DUMMY_SP);
        directive_arr.elems.push(Some(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(name)),
        }));

        // Tries to early exit if we reached the desired array length
        macro_rules! early_exit {
            ($desired: expr) => {
                if directive_arr_len_hint == $desired {
                    return Expr::Array(directive_arr);
                }
            };
        }

        early_exit!(1);

        // Write the value or `void 0`
        directive_arr.elems.push(Some(ExprOrSpread {
            spread: None,
            expr: if let Some(directive_value) = value {
                Box::new(directive_value.to_owned())
            } else {
                Box::new(void0())
            },
        }));

        early_exit!(2);

        // Write the argument or `void 0`
        directive_arr.elems.push(Some(ExprOrSpread {
            spread: None,
            expr: if let Some(argument) = argument {
                Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: JsWord::from(argument),
                    raw: None,
                })))
            } else {
                Box::new(void0())
            },
        }));

        early_exit!(3);

        // Write the modifiers object in form `{ mod1: true, mod2: true }`
        let mut modifiers_obj = ObjectLit {
            span: DUMMY_SP,
            props: Vec::with_capacity(modifiers.len()),
        };
        for modifier in modifiers.iter() {
            modifiers_obj
                .props
                .push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: str_to_propname(&modifier, DUMMY_SP),
                    value: Box::new(Expr::Lit(Lit::Bool(Bool {
                        span: DUMMY_SP,
                        value: true,
                    }))),
                }))))
        }
        directive_arr.elems.push(Some(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Object(modifiers_obj)),
        }));

        Expr::Array(directive_arr)
    }

    fn get_custom_directive_ident(&mut self, directive_name: &str, span: Span) -> Ident {
        // Check directive existence and early exit
        let existing_directive_name = self.directives.get(directive_name);
        if let Some(directive_name) = existing_directive_name {
            return Ident {
                span,
                sym: directive_name.to_owned(),
                optional: false,
            };
        }

        // _directive_ prefix plus directive name
        let mut directive_ident_raw = directive_name.replace('-', "_");
        directive_ident_raw.insert_str(0, "_directive_");
        let directive_ident_atom = JsWord::from(directive_ident_raw);

        // Add to map
        self.directives
            .insert(directive_name.to_owned(), directive_ident_atom.to_owned());

        Ident {
            span,
            sym: directive_ident_atom,
            optional: false,
        }
    }

    fn generate_directive_resolves(&mut self) -> Vec<VarDeclarator> {
        let mut result = Vec::new();

        if self.directives.len() == 0 {
            return result;
        }

        let resolve_directive_ident = self.get_and_add_import_ident(VueImports::ResolveDirective);

        // We need sorted entries for stable output.
        // Entries are sorted by Js identifier (second element of tuple in hashmap entry)
        let mut sorted_directives: Vec<(&str, &JsWord)> = self
            .directives
            .iter()
            .map(|(directive_name, directive_ident)| (directive_name.as_str(), directive_ident))
            .collect();

        sorted_directives.sort_by(|a, b| a.1.cmp(b.1));

        // Key is a component as used in template, value is the assigned Js identifier
        for (directive_name, identifier) in sorted_directives.iter() {
            // _directive_ident_name = resolveDirective("directive-template-name")
            result.push(VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident {
                        span: DUMMY_SP,
                        sym: (*identifier).to_owned(),
                        optional: false,
                    },
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                        span: DUMMY_SP,
                        sym: resolve_directive_ident.to_owned(),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Lit(Lit::Str(Str {
                            span: DUMMY_SP,
                            value: JsWord::from(*directive_name),
                            raw: None,
                        }))),
                    }],
                    type_args: None,
                }))),
                definite: false,
            });
        }

        result
    }
}

/// Generates `void 0` expression
fn void0() -> Expr {
    Expr::Unary(UnaryExpr {
        span: DUMMY_SP,
        op: UnaryOp::Void,
        arg: Box::new(Expr::Lit(Lit::Num(Number {
            raw: None,
            span: DUMMY_SP,
            value: 0.0,
        }))),
    })
}
