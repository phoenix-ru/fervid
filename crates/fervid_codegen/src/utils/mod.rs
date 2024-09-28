// to_ident_or_str

use std::fmt::{Write, Error};

use fervid_core::{FervidAtom, StrOrExpr};
use swc_core::{common::Span, ecma::ast::{ComputedPropName, Ident, EsReserved, IdentName, PropName, Str}};

/// Adapted from SWC Ident::verify_symbol
#[inline]
pub fn is_valid_ident(s: &str) -> bool {
    if s.is_reserved() || s.is_reserved_in_strict_mode(true) || s.is_reserved_in_strict_bind() {
        return false;
    }

    let mut chars = s.chars();

    if let Some(first) = chars.next() {
        if Ident::is_valid_start(first) && chars.all(Ident::is_valid_continue) {
            return true;
        }
    }

    false
}

pub fn str_to_propname(s: &str, span: Span) -> PropName {
    if is_valid_ident(s) {
        PropName::Ident(IdentName { span, sym: s.into() })
    } else {
        PropName::Str(Str {
            span,
            value: s.into(),
            raw: None,
        })
    }
}

pub fn atom_to_propname(sym: FervidAtom, span: Span) -> PropName {
    if is_valid_ident(&sym) {
        PropName::Ident(IdentName { span, sym })
    } else {
        PropName::Str(Str {
            span,
            value: sym,
            raw: None,
        })
    }
}

pub fn str_or_expr_to_propname(str_or_expr: StrOrExpr, span: Span) -> PropName {
    match str_or_expr {
        StrOrExpr::Str(sym) => atom_to_propname(sym, span),
        StrOrExpr::Expr(expr) => PropName::Computed(ComputedPropName {
            span,
            expr,
        }),
    }
}

pub fn to_camelcase(s: &str, buf: &mut impl Write) -> Result<(), Error> {
    for (idx, word) in s.split('-').enumerate() {
        if idx == 0 {
            buf.write_str(word)?;
            continue;
        }

        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                buf.write_char(ch_component)?;
            }

            // Push the rest of the word
            buf.write_str(&word[ch.len_utf8()..])?;
        }
    }

    Ok(())
}
