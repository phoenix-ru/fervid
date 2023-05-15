use swc_core::{
    common::{Span, SyntaxContext, BytePos},
    ecma::ast::{EsVersion, Module},
};
use swc_ecma_parser::{lexer::Lexer, EsConfig, Parser, StringInput, Syntax, TsConfig};

pub fn parse_javascript_module(
    input: &str,
    span_start: u32,
    es_config: EsConfig,
) -> Result<Module, swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
        SyntaxContext::empty(),
    );

    let lexer = Lexer::new(
        Syntax::Es(es_config),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_module()
}

pub fn parse_typescript_module(
    input: &str,
    span_start: u32,
    ts_config: TsConfig,
) -> Result<Module, swc_ecma_parser::error::Error> {
    let span = Span::new(
        BytePos(span_start),
        BytePos(span_start + input.len() as u32),
        SyntaxContext::empty(),
    );

    let lexer = Lexer::new(
        Syntax::Typescript(ts_config),
        EsVersion::EsNext,
        StringInput::new(input, span.lo, span.hi),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_typescript_module()
}
