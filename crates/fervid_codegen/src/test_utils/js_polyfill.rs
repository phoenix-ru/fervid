use swc_core::{
    common::BytePos,
    ecma::ast::{Expr, Pat},
};
use swc_ecma_parser::{PResult, Parser, StringInput, Syntax, lexer::Lexer};

/// Parses JS into an Expr
pub fn parse_js_into_expr(expr: &str) -> PResult<Box<Expr>> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(expr, BytePos(0), BytePos(expr.len() as u32)),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_expr()
}

/// Parses JS into a Pat
pub fn parse_js_into_pat(pat: &str) -> PResult<Pat> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(pat, BytePos(0), BytePos(pat.len() as u32)),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_pat()
}
