use swc_core::{
    common::{comments::SingleThreadedComments, BytePos, Span},
    ecma::ast::{EsVersion, Expr, Module},
};
use swc_ecma_parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax, TsSyntax};

pub fn parse_javascript_module(
    input: &str,
    span_start: u32,
    es_syntax: EsSyntax,
) -> Result<(Module, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Es(es_syntax),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_module().map(|module| (module, comments))
}

pub fn parse_typescript_module(
    input: &str,
    span_start: u32,
    ts_syntax: TsSyntax,
) -> Result<(Module, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Typescript(ts_syntax),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    parser
        .parse_typescript_module()
        .map(|module| (module, comments))
}

pub fn parse_javascript_expr(
    input: &str,
    span_start: u32,
    es_syntax: EsSyntax,
) -> Result<(Box<Expr>, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Es(es_syntax),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_expr().map(|module| (module, comments))
}

pub fn parse_typescript_expr(
    input: &str,
    span_start: u32,
    ts_syntax: TsSyntax,
) -> Result<(Box<Expr>, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Typescript(ts_syntax),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_expr().map(|module| (module, comments))
}
