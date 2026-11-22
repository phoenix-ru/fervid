use swc_core::{
    common::BytePos,
    ecma::ast::{Expr, Module, Pat},
};
use swc_ecma_parser::{PResult, Parser, StringInput, Syntax, lexer::Lexer};

pub fn parse_js(raw: &str, span_start: u32, span_end: u32) -> PResult<Box<Expr>> {
    // let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        string_input(raw, span_start, span_end),
        // Some(&comments),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    // TODO Return comments or use parser instance to store it
    parser.parse_expr()
}

pub fn parse_js_module(raw: &str, span_start: u32, span_end: u32) -> PResult<Module> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        string_input(raw, span_start, span_end),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    // TODO Return comments or use parser instance to store it
    parser.parse_module()
}

pub fn parse_js_pat(raw: &str, span_start: u32, span_end: u32) -> PResult<Pat> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        string_input(raw, span_start, span_end),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    // TODO Return comments or use parser instance to store it
    parser.parse_pat()
}

fn string_input(raw: &str, span_start: u32, span_end: u32) -> StringInput<'_> {
    // Legacy call sites used `0` as a shorthand for the whole source.
    let end = if span_end == 0 {
        let raw_len = raw.len().min(u32::MAX as usize) as u32;
        span_start.saturating_add(raw_len)
    } else {
        span_end
    };

    StringInput::new(raw, BytePos(span_start), BytePos(end))
}
