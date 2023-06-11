use swc_core::{ecma::{ast::{Expr, Lit, Str}, atoms::{JsWord, Atom}}, common::Span};

use crate::context::CodegenContext;

impl CodegenContext {
    pub fn generate_text_node(&mut self, contents: &str, span: Span) -> Expr {
        Expr::Lit(Lit::Str(Str {
            span,
            value: JsWord::from(contents),
            raw: Some(Atom::from(contents)),
        }))
    }
}
