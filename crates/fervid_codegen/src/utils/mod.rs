use std::fmt::{Error, Write};

use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, ExprOrSpread, Lit, Null},
};

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

pub fn expr_arg(expr: Expr) -> ExprOrSpread {
    ExprOrSpread {
        spread: None,
        expr: Box::new(expr),
    }
}

/// Produces a `null` expression
pub fn null_expr() -> Expr {
    Expr::Lit(Lit::Null(Null { span: DUMMY_SP }))
}
