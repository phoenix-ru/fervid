use swc_core::{
    common::{comments::SingleThreadedComments, BytePos, Span, SyntaxContext},
    ecma::ast::{EsVersion, Module, Expr},
};
use swc_ecma_parser::{lexer::Lexer, EsConfig, Parser, StringInput, Syntax, TsConfig};

pub fn parse_javascript_module(
    input: &str,
    span_start: u32,
    es_config: EsConfig,
) -> Result<(Module, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
        SyntaxContext::empty(),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Es(es_config),
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
    ts_config: TsConfig,
) -> Result<(Module, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
        SyntaxContext::empty(),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Typescript(ts_config),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_typescript_module().map(|module| (module, comments))
}

pub fn parse_javascript_expr(
    input: &str,
    span_start: u32,
    es_config: EsConfig,
) -> Result<(Box<Expr>, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
        SyntaxContext::empty(),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Es(es_config),
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
    ts_config: TsConfig,
) -> Result<(Box<Expr>, SingleThreadedComments), swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
        SyntaxContext::empty(),
    );

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Typescript(ts_config),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_expr().map(|module| (module, comments))
}
