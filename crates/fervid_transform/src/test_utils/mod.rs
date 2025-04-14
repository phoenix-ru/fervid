pub mod parser;

use swc_core::common::{sync::Lrc, SourceMap};
use swc_core::ecma::ast::Expr;
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};

use self::parser::{parse_javascript_expr, parse_typescript_expr};

pub fn js(raw: &str) -> Box<Expr> {
    parse_javascript_expr(raw, 0, Default::default()).unwrap().0
}

pub fn ts(raw: &str) -> Box<Expr> {
    parse_typescript_expr(raw, 0, Default::default()).unwrap().0
}

pub fn to_str(swc_node: &impl Node) -> String {
    // Emitting the result requires some setup with SWC
    let cm: Lrc<SourceMap> = Default::default();
    let mut buff: Vec<u8> = Vec::with_capacity(128);
    let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

    let mut emitter_cfg = swc_ecma_codegen::Config::default();
    emitter_cfg.minify = true;

    let mut emitter = Emitter {
        cfg: emitter_cfg,
        comments: None,
        wr: writer,
        cm,
    };

    let _ = swc_node.emit_with(&mut emitter);

    String::from_utf8(buff).unwrap()
}

#[macro_export]
macro_rules! span {
    ($lo: expr, $hi: expr) => {
        Span::new(BytePos($lo), BytePos($hi))
    };
}
