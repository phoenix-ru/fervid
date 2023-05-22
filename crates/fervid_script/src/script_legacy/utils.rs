use swc_core::ecma::{
    ast::{
        BlockStmt, Callee, Expr, Function, Module, ModuleDecl, ModuleItem, ObjectLit, Prop,
        PropName, PropOrSpread, ReturnStmt, Stmt, ArrayLit, ExprOrSpread, Lit,
    },
    atoms::JsWord,
};

pub fn find_default_export(module: &Module) -> Option<&ObjectLit> {
    let define_component = JsWord::from("defineComponent");

    module.body.iter().rev().find_map(|module_item| {
        let ModuleItem::ModuleDecl(module_decl) = module_item else {
            return None;
        };

        match module_decl {
            ModuleDecl::ExportDefaultExpr(export_default_expr) => {
                match *export_default_expr.expr {
                    // Plain export
                    // `export default { /* ... */ }`
                    Expr::Object(ref object_lit) => {
                        return Some(object_lit);
                    }

                    // When doing `defineComponent`
                    Expr::Call(ref call_expr) => {
                        let Callee::Expr(ref callee_expr) = call_expr.callee else {
                            return None;
                        };

                        // Only support unwrapping `defineComponent`
                        if let Expr::Ident(ref ident) = **callee_expr {
                            if ident.sym != define_component {
                                return None;
                            }
                        } else {
                            return None;
                        };

                        let Some(first_arg) = call_expr.args.get(0) else {
                            return None;
                        };

                        match *first_arg.expr {
                            Expr::Object(ref object_lit) => Some(object_lit),
                            _ => None,
                        }
                    }

                    _ => None,
                }
            }
            _ => None,
        }
    })
}

pub fn find_function(object: &ObjectLit, name: JsWord) -> Option<&Function> {
    object.props.iter().find_map(|prop| match prop {
        PropOrSpread::Prop(prop) => {
            let Prop::Method(ref method) = **prop else {
                return None;
            };

            match method.key {
                PropName::Ident(ref ident) if ident.sym == name => Some(&*method.function),

                PropName::Str(ref s) if s.value == name => Some(&*method.function),

                _ => None,
            }
        }
        _ => None,
    })
}

pub fn find_return(block_stmt: &BlockStmt) -> Option<&ReturnStmt> {
    block_stmt.stmts.iter().find_map(|stmt| match stmt {
        Stmt::Return(ref return_stmt) => Some(return_stmt),

        _ => None,
    })
}

pub fn collect_block_stmt_return_fields(block_stmt: &BlockStmt, out: &mut Vec<JsWord>) {
    let Some(return_stmt) = find_return(block_stmt) else {
        return;
    };

    let Some(ref return_arg) = return_stmt.arg else {
        return;
    };

    let return_arg = unroll_paren_seq(&return_arg);

    let Expr::Object(ref return_obj) = *return_arg else {
        return;
    };

    collect_obj_fields(return_obj, out);
}

pub fn collect_obj_fields(object: &ObjectLit, out: &mut Vec<JsWord>) {
    for prop in object.props.iter() {
        collect_obj_prop_or_spread(prop, out)
    }
}

pub fn collect_obj_prop_or_spread(prop_or_spread: &PropOrSpread, out: &mut Vec<JsWord>) {
    let PropOrSpread::Prop(prop) = prop_or_spread else {
        return;
    };

    match **prop {
        Prop::Shorthand(ref ident) => {
            out.push(ident.sym.to_owned());
        }

        Prop::KeyValue(ref key_value) => {
            collect_obj_propname(&key_value.key, out)
        }

        Prop::Method(ref method) => {
            collect_obj_propname(&method.key, out)
        }

        // Prop::Assign(_) => todo!(),
        // Prop::Getter(_) => todo!(),
        // Prop::Setter(_) => todo!(),
        _ => {}
    };
}

#[inline]
pub fn collect_obj_propname(prop_name: &PropName, out: &mut Vec<JsWord>) {
    match prop_name {
        PropName::Ident(ref ident) => out.push(ident.sym.to_owned()),
        PropName::Str(ref s) => {
            out.push(s.value.to_owned())
        }

        // I am not really sure how computed keys (e.g. `foo` in `{ [foo]: bar }`)
        // should be recognized. I believe they should not.
        // PropName::Computed(_) => todo!()

        _ => {}
    }
}

/// Collects all the string literals from a `string[]`
pub fn collect_string_arr(arr: &ArrayLit, out: &mut Vec<JsWord>) {
    // We expect to collect all the props
    out.reserve(arr.elems.len());

    for elem in arr.elems.iter() {
        // I don't understand why this is an option though
        let Some(ExprOrSpread { spread: None, expr }) = elem else {
            continue;
        };

        // Only string literals are supported in array syntax
        // We do not dedupe anything in general
        match **expr {
            Expr::Lit(Lit::Str(ref s)) => {
                out.push(s.value.to_owned())
            }
            // Js template string: `foo` (with backticks)
            Expr::Tpl(ref tpl) => {
                // This is not a js runtime, only simple template strings are supported
                if tpl.exprs.len() > 0 || tpl.quasis.len() != 1 {
                    continue;
                };

                let Some(template_elem) = tpl.quasis.get(0) else {
                    continue;
                };

                let Some(ref template_string) = template_elem.cooked else {
                    continue;
                };

                out.push(template_string.as_ref().into())
            }
            _ => {}
        }
    }
}

/// Unrolls an expression from parenthesis and sequences.
/// This is usable for arrow functions like `() => ({})`,
/// where we need to get the `{}` part.
/// Also unrolls the sequence syntax, as it is a legal JS: `a, "b", 42` -> `42`.
///
/// ## Example
/// This function works recursively. `(a, ("b", 42))` -> `42`
/// ```
/// # use swc_core::{
/// #     common::DUMMY_SP,
/// #     ecma::{
/// #         ast::{Ident, Expr, Lit, Number, ParenExpr, SeqExpr, Str},
/// #         atoms::JsWord,
/// #     }
/// # };
/// # use fervid_script::script_legacy::utils::unroll_paren_seq;
/// let expr = Expr::Paren(ParenExpr {
///     span: DUMMY_SP,
///     expr: Expr::Seq(SeqExpr {
///         exprs: vec![
///             Expr::Ident(Ident::new(JsWord::from("a"), DUMMY_SP)).into(),
///             Expr::Paren(ParenExpr {
///                 expr: Expr::Seq(SeqExpr {
///                     span: DUMMY_SP,
///                     exprs: vec![
///                         Expr::Lit(Lit::Str(Str::from(JsWord::from("b")))).into(),
///                         Expr::Lit(Lit::Num(Number {
///                             span: DUMMY_SP,
///                             value: 42.0,
///                             raw: None,
///                         }))
///                         .into(),
///                     ],
///                 })
///                 .into(),
///                 span: DUMMY_SP,
///             }).into()
///         ],
///         span: DUMMY_SP,
///     })
///     .into(),
/// });
/// 
/// let expr = unroll_paren_seq(&expr);
///
/// let Expr::Lit(lit) = expr else {
///     panic!("Not a literal!")
/// };
/// 
/// let Lit::Num(num) = lit else {
///     panic!("Not a number!");
/// };
/// assert_eq!(num.value, 42.0);
/// ```
pub fn unroll_paren_seq(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren_expr) => unroll_paren_seq(&paren_expr.expr),

        // Afaik, `SeqExpr` always has elements in it.
        // If that was not the case, this arm won't be matched and `expr` will be returned instead.
        // The consumer will have to handle this weird `SeqExpr` himself.
        Expr::Seq(seq_expr) if seq_expr.exprs.len() > 0 => {
            unroll_paren_seq(&seq_expr.exprs[seq_expr.exprs.len() - 1])
        }

        _ => expr,
    }
}
