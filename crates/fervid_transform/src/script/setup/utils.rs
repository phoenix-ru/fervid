use fervid_core::{FervidAtom, IntoIdent};
use itertools::Itertools;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{ArrayLit, Expr, ExprOrSpread},
};

use crate::script::resolve_type::TypesSet;

pub fn to_runtime_type_string(types: TypesSet) -> Box<Expr> {
    let mut idents: Vec<&'static str> = Vec::new();

    for type_name in types.into_iter() {
        idents.push(type_name.into())
    }

    if idents.len() == 1 {
        return Box::new(Expr::Ident(FervidAtom::from(idents[0]).into_ident()));
    }

    let array_elems = idents
        .into_iter()
        .map(|ident| {
            Some(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(FervidAtom::from(ident).into_ident())),
            })
        })
        .collect_vec();

    Box::new(Expr::Array(ArrayLit {
        span: DUMMY_SP,
        elems: array_elems,
    }))
}

pub fn unwrap_ts_node_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::TsConstAssertion(ts_const_assertion) => unwrap_ts_node_expr(&ts_const_assertion.expr),
        Expr::TsNonNull(ts_non_null_expr) => unwrap_ts_node_expr(&ts_non_null_expr.expr),
        Expr::TsAs(ts_as_expr) => unwrap_ts_node_expr(&ts_as_expr.expr),
        Expr::TsInstantiation(ts_instantiation) => unwrap_ts_node_expr(&ts_instantiation.expr),
        Expr::TsSatisfies(ts_satisfies_expr) => unwrap_ts_node_expr(&ts_satisfies_expr.expr),
        _ => expr
    }
}
