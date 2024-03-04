use fervid_core::FervidAtom;
use swc_core::{common::DUMMY_SP, ecma::ast::{ArrowExpr, BindingIdent, BlockStmtOrExpr, Expr, Ident, Pat}};

/// `foo-bar-baz` -> `FooBarBaz`
#[inline]
pub(crate) fn to_pascal_case(raw: &str, out: &mut String) {
    for word in raw.split('-') {
        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                out.push(ch_component);
            }

            // Push the rest of the word
            out.push_str(&word[ch.len_utf8()..]);
        }
    }
}

/// `foo-bar-baz` -> `fooBarBaz`
#[inline]
pub(crate) fn to_camel_case(raw: &str, out: &mut String) {
    for (idx, word) in raw.split('-').enumerate() {
        if idx == 0 {
            out.push_str(word);
            continue;
        }

        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                out.push(ch_component);
            }

            // Push the rest of the word
            out.push_str(&word[ch.len_utf8()..]);
        }
    }
}

/// Wraps `expr` to `$event => (expr)`
#[inline]
pub fn wrap_in_event_arrow(expr: Box<Expr>) -> Box<Expr> {
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
