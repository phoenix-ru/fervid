use swc_core::{
    common::Span,
    ecma::ast::{ComputedPropName, EsReserved, Ident, IdentName, PropName, Str},
};

use crate::{AttributeOrBinding, FervidAtom, StrOrExpr, VBindDirective};

/// Checks whether the attributes name is the same as `expected_name`
#[inline]
pub fn check_attribute_name(attr: &AttributeOrBinding, expected_name: &str) -> bool {
    matches!(attr,
        AttributeOrBinding::RegularAttribute { name, .. } |
        AttributeOrBinding::VBind(VBindDirective { argument: Some(StrOrExpr::Str(name)), .. })
        if name == expected_name
    )
}

/// Adapted from SWC Ident::verify_symbol
#[inline]
pub fn is_valid_ident(s: &str) -> bool {
    if s.is_reserved() || s.is_reserved_in_strict_mode(true) || s.is_reserved_in_strict_bind() {
        return false;
    }

    is_valid_propname(s)
}

#[inline]
pub fn is_valid_propname(s: &str) -> bool {
    let mut chars = s.chars();

    if let Some(first) = chars.next() {
        if Ident::is_valid_start(first) && chars.all(Ident::is_valid_continue) {
            return true;
        }
    }

    false
}

pub fn str_to_propname(s: &str, span: Span) -> PropName {
    if is_valid_propname(s) {
        PropName::Ident(IdentName {
            span,
            sym: s.into(),
        })
    } else {
        PropName::Str(Str {
            span,
            value: s.into(),
            raw: None,
        })
    }
}

pub fn atom_to_propname(sym: FervidAtom, span: Span) -> PropName {
    if is_valid_propname(&sym) {
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
        StrOrExpr::Expr(expr) => PropName::Computed(ComputedPropName { span, expr }),
    }
}
