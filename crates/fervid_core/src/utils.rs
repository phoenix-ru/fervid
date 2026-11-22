use std::{
    borrow::Borrow,
    fmt::{self, Write},
    hash::{Hash, Hasher},
};

use fxhash::FxHasher32;
use swc_core::{
    common::Span,
    ecma::{
        ast::{ComputedPropName, EsReserved, Ident, IdentName, PropName, Str},
        atoms::Wtf8Atom,
    },
};

use crate::{AttributeOrBinding, FervidAtom, StrOrExpr, VBindDirective};

/// Checks whether the attributes name is the same as `expected_name`
#[inline]
pub fn check_attribute_name(attr: &AttributeOrBinding, expected_name: &str) -> bool {
    match attr {
        AttributeOrBinding::RegularAttribute { name, .. } => name == expected_name,
        AttributeOrBinding::VBind(VBindDirective {
            argument: Some(StrOrExpr::Str(name)),
            ..
        }) => name.value == expected_name,
        _ => false,
    }
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

    if let Some(first) = chars.next()
        && Ident::is_valid_start(first)
        && chars.all(Ident::is_valid_continue)
    {
        return true;
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
        // Valid UTF-8 string and valid ident
        PropName::Ident(IdentName { span, sym })
    } else {
        // Everything else
        PropName::Str(Str {
            span,
            value: sym.into(),
            raw: None,
        })
    }
}

pub fn wtf8_atom_to_propname(sym: Wtf8Atom, span: Span) -> PropName {
    match sym.try_into_atom() {
        Ok(atom) => atom_to_propname(atom, span),
        Err(value) => PropName::Str(Str {
            span,
            value,
            raw: None,
        }),
    }
}

pub fn str_or_expr_to_propname(str_or_expr: StrOrExpr, span: Span) -> PropName {
    match str_or_expr {
        StrOrExpr::Str(s) => atom_to_propname(s.value, s.span),
        StrOrExpr::Expr(expr) => PropName::Computed(ComputedPropName { span, expr }),
    }
}

/// Converts a WTF-8 atom to a UTF-8 atom. Invalid sequences are replaced
/// because downstream Rust APIs are UTF-8-only; generated JS string values
/// should keep the original `Wtf8Atom` when possible.
pub fn wtf8_atom_to_atom(value: &Wtf8Atom) -> FervidAtom {
    value
        .as_atom()
        .cloned()
        .unwrap_or_else(|| FervidAtom::from(value.to_string_lossy()))
}

pub fn wtf8_atom_to_string(value: &Wtf8Atom) -> String {
    match value.as_str() {
        Some(value) => value.to_owned(),
        None => value.to_string_lossy().into_owned(),
    }
}

/// Borrows the UTF-8 atom as WTF-8 to retain native atom equality fast paths
#[inline]
pub fn wtf8_atom_matches_atom(value: &Wtf8Atom, atom: &FervidAtom) -> bool {
    value == <FervidAtom as Borrow<Wtf8Atom>>::borrow(atom)
}

#[inline]
pub fn wtf8_atom_matches_str(value: &Wtf8Atom, expected: &str) -> bool {
    value.as_bytes() == expected.as_bytes()
}

/// Matches the existing numeric key Display spelling without allocating a String
pub fn wtf8_atom_matches_number(value: &Wtf8Atom, expected: f64) -> bool {
    struct KeyComparator<'a>(&'a [u8]);

    impl Write for KeyComparator<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let Some(remaining) = self.0.strip_prefix(s.as_bytes()) else {
                return Err(fmt::Error);
            };
            self.0 = remaining;
            Ok(())
        }
    }

    let mut comparator = KeyComparator(value.as_bytes());
    write!(&mut comparator, "{expected}").is_ok() && comparator.0.is_empty()
}

#[inline]
pub fn wtf8_atom_ends_with_str(value: &Wtf8Atom, suffix: &str) -> bool {
    value.as_bytes().ends_with(suffix.as_bytes())
}

pub fn compute_scope_id(source: &str) -> String {
    let mut hasher = FxHasher32::default();
    source.hash(&mut hasher);
    let num = hasher.finish();
    format!("{:x}", num)
}
