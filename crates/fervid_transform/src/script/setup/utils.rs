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
