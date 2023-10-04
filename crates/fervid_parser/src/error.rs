use swc_core::common::{Span, Spanned};

#[derive(Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum ParseErrorKind {
    /// Malformed directive (e.g. `:`, `@`)
    DirectiveSyntax,
    /// Unclosed dynamic argument, e.g. `:[dynamic`
    DynamicArgument,
    /// Error while parsing EcmaScript/TypeScript
    BadExpr(swc_ecma_parser::error::SyntaxError),
    /// Unrecoverable error while parsing HTML
    InvalidHtml(swc_html_parser::error::ErrorKind),
}

impl From<swc_ecma_parser::error::Error> for ParseError {
    fn from(value: swc_ecma_parser::error::Error) -> ParseError {
        let span = value.span();

        ParseError {
            kind: ParseErrorKind::BadExpr(value.into_kind()),
            span,
        }
    }
}

impl std::fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
