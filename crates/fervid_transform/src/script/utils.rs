//! A collection of utils for working with SWC structs

use fervid_core::{fervid_atom, FervidAtom};
use swc_core::ecma::ast::{
    ArrayLit, BlockStmt, Callee, Expr, ExprOrSpread, Function, Lit, Module, ObjectLit, Prop,
    PropName, PropOrSpread, ReturnStmt, Stmt, Tpl,
};

#[deprecated]
pub fn find_default_export(module: &Module) -> Option<&ObjectLit> {
    let define_component = fervid_atom!("defineComponent");

    module.body.iter().rev().find_map(|module_item| {
        let module_decl = module_item.as_module_decl()?;
        let export_default_expr = module_decl.as_export_default_expr()?;

        match *export_default_expr.expr {
            // Plain export
            // `export default { /* ... */ }`
            Expr::Object(ref object_lit) => Some(object_lit),

            // When doing `defineComponent`
            Expr::Call(ref call_expr) => {
                let callee_expr = call_expr.callee.as_expr()?;

                // Only support unwrapping `defineComponent`
                let ident = callee_expr.as_ident()?;
                if ident.sym != define_component {
                    return None;
                }

                let first_arg = call_expr.args.first()?;
                first_arg.expr.as_object()
            }

            _ => None,
        }
    })
}

/// Finds a function in the object by a given name
pub fn find_function(object: &ObjectLit, name: FervidAtom) -> Option<&Function> {
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

/// Finds the return statement in the [BlockStmt]. The search will occur from the statement end.
pub fn find_return(block_stmt: &BlockStmt) -> Option<&ReturnStmt> {
    block_stmt.stmts.iter().rev().find_map(|stmt| match stmt {
        Stmt::Return(ref return_stmt) => Some(return_stmt),

        _ => None,
    })
}

/// Collects all the field keys in the return object of a [BlockStmt]
pub fn collect_block_stmt_return_fields(block_stmt: &BlockStmt, out: &mut Vec<FervidAtom>) {
    let Some(return_stmt) = find_return(block_stmt) else {
        return;
    };

    let Some(ref return_arg) = return_stmt.arg else {
        return;
    };

    let return_arg = unroll_paren_seq(return_arg);

    let Expr::Object(ref return_obj) = *return_arg else {
        return;
    };

    collect_obj_fields(return_obj, out);
}

/// Collects all the field keys of the object
pub fn collect_obj_fields(object: &ObjectLit, out: &mut Vec<FervidAtom>) {
    for prop in object.props.iter() {
        collect_obj_prop_or_spread(prop, out)
    }
}

/// Collects the single field of an object
pub fn collect_obj_prop_or_spread(prop_or_spread: &PropOrSpread, out: &mut Vec<FervidAtom>) {
    let PropOrSpread::Prop(prop) = prop_or_spread else {
        return;
    };

    match **prop {
        Prop::Shorthand(ref ident) => {
            out.push(ident.sym.to_owned());
        }

        Prop::KeyValue(ref key_value) => collect_obj_propname(&key_value.key, out),

        Prop::Method(ref method) => collect_obj_propname(&method.key, out),

        // Prop::Assign(_) => todo!(),
        // Prop::Getter(_) => todo!(),
        // Prop::Setter(_) => todo!(),
        _ => {}
    };
}

/// Collects the property name of an object, e.g. `foo` in `{ foo: 'bar' }`
#[inline]
pub fn collect_obj_propname(prop_name: &PropName, out: &mut Vec<FervidAtom>) {
    match prop_name {
        PropName::Ident(ref ident) => out.push(ident.sym.to_owned()),
        PropName::Str(ref s) => out.push(s.value.to_owned()),

        // I am not really sure how computed keys (e.g. `foo` in `{ [foo]: bar }`)
        // should be recognized. I believe they should not.
        // PropName::Computed(_) => todo!()
        _ => {}
    }
}

/// Collects all the string literals from a `string[]`
pub fn collect_string_arr(arr: &ArrayLit, out: &mut Vec<FervidAtom>) {
    // We expect to collect all the props
    out.reserve(arr.elems.len());

    for elem in arr.elems.iter() {
        // I don't understand why this is an option though
        let Some(ExprOrSpread { spread: None, expr }) = elem else {
            continue;
        };

        // Only string literals are supported in array syntax
        let Some(s) = get_string_expr(expr) else {
            continue;
        };

        // We do not dedupe anything in general
        out.push(s)
    }
}

/// Gets a `string` value from expr, either `'literal'` or from <code>\`template string\`</code>
pub fn get_string_expr(expr: &Expr) -> Option<FervidAtom> {
    match *expr {
        Expr::Lit(Lit::Str(ref s)) => Some(s.value.to_owned()),

        // Js template string: `foo` (with backticks)
        Expr::Tpl(ref tpl) => get_string_tpl(tpl),

        _ => None,
    }
}

/// Gets the template string if it is simple:
/// - <code>\`something simple\`</code> is trivial and returns `Some(FervidAtom::from("something simple"))`;
/// - <code>\`something ${notSoSimple}\`</code> is not trivial and will return `None`.
pub fn get_string_tpl(tpl: &Tpl) -> Option<FervidAtom> {
    // This is not a js runtime, only simple template strings are supported
    if !tpl.exprs.is_empty() || tpl.quasis.len() != 1 {
        return None;
    };

    let template_elem = tpl.quasis.first()?;

    let template_string = template_elem.cooked.as_ref()?;

    Some(template_string.to_owned())
}

/// Checks if the expression only contains literals
/// https://github.com/vuejs/core/blob/d276a4f3e914aaccc291f7b2513e5d978919d0f9/packages/compiler-sfc/src/compileScript.ts#L1228
pub fn is_static(expr: &Expr) -> bool {
    match expr {
        Expr::Unary(unary) => is_static(&unary.arg),

        Expr::Bin(bin) => is_static(&bin.left) && is_static(&bin.right),

        Expr::Cond(cond) => is_static(&cond.test) && is_static(&cond.cons) && is_static(&cond.alt),

        Expr::Seq(seq) => seq.exprs.iter().all(|e| is_static(e)),

        Expr::Tpl(tpl) => tpl.exprs.iter().all(|e| is_static(e)),

        Expr::Paren(paren) => is_static(&paren.expr),

        Expr::Lit(_) => true,

        _ => false,
    }
}

/// https://github.com/vuejs/core/blob/466b30f4049ec89fb282624ec17d1a93472ab93f/packages/compiler-sfc/src/script/utils.ts#L15-L27
pub fn resolve_object_key(key: &PropName) -> Option<FervidAtom> {
    match key {
        PropName::Ident(ident_name) => Some(ident_name.sym.to_owned()),
        PropName::Str(s) => Some(s.value.to_owned()),
        PropName::Num(number) => Some(number.value.to_string().into()),
        PropName::Computed(computed_prop_name) => {
            match computed_prop_name.expr.as_ref() {
                // This is considered StringLiteral or NumericLiteral in Babel,
                // meaning that `const { ['foo']: foo }` and `const { 'foo': foo }` are the same in Babel,
                // but not in SWC
                Expr::Lit(lit) => match lit {
                    Lit::Str(s) => Some(s.value.to_owned()),
                    Lit::Num(number) => number
                        .raw
                        .to_owned()
                        .or_else(|| Some(number.value.to_string().into())),

                    _ => None,
                },

                _ => None,
            }
        }
        PropName::BigInt(_) => None,
    }
}

/// https://github.com/vuejs/core/blob/32bc647faba56f50a37d18b08fcc0e11b49c791f/packages/compiler-sfc/src/script/utils.ts#L39-L52
pub fn is_call_of(expr: &Expr, test: &FervidAtom) -> bool {
    let Expr::Call(ref call_expr) = expr else {
        return false;
    };

    let Callee::Expr(ref callee_expr) = call_expr.callee else {
        return false;
    };

    let Expr::Ident(ref callee_ident) = callee_expr.as_ref() else {
        return false;
    };

    &callee_ident.sym == test
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
/// #     ecma::ast::{Ident, Expr, Lit, Number, ParenExpr, SeqExpr, Str},
/// # };
/// # use fervid_transform::script::utils::unroll_paren_seq;
/// # use fervid_core::FervidAtom;
///
/// let expr = Expr::Paren(ParenExpr {
///     span: DUMMY_SP,
///     expr: Expr::Seq(SeqExpr {
///         exprs: vec![
///             Expr::Ident(Ident::new(FervidAtom::from("a"), DUMMY_SP, Default::default())).into(),
///             Expr::Paren(ParenExpr {
///                 expr: Expr::Seq(SeqExpr {
///                     span: DUMMY_SP,
///                     exprs: vec![
///                         Expr::Lit(Lit::Str(Str::from(FervidAtom::from("b")))).into(),
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
        Expr::Seq(seq_expr) if !seq_expr.exprs.is_empty() => {
            unroll_paren_seq(&seq_expr.exprs[seq_expr.exprs.len() - 1])
        }

        _ => expr,
    }
}
