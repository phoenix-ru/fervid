// to_ident_or_str

use std::fmt::{Write, Error};

use swc_core::{ecma::ast::{Ident, IdentExt, PropName, Str}, common::Span};

mod all_html_tags;

pub use all_html_tags::is_html_tag;

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
        PropName::Ident(Ident { span, sym: s.into(), optional: false })
    } else {
        PropName::Str(Str {
            span,
            value: s.into(),
            raw: Some(s.into()),
        })
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

pub fn to_pascalcase(s: &str, buf: &mut impl Write) -> Result<(), Error> {
    for word in s.split('-') {
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
