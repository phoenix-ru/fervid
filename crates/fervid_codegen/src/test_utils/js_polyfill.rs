use swc_core::{common::BytePos, ecma::ast::Expr};
use swc_ecma_parser::{lexer::Lexer, PResult, Parser, StringInput, Syntax};

/// Parses js as a temporary measure
pub fn parse_js(expr: &str) -> PResult<Box<Expr>> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(expr, BytePos(0), BytePos(0)),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    parser.parse_expr()
}
