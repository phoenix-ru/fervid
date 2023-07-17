use std::sync::Arc;

use swc_core::{common::SourceMap, ecma::ast::Expr};
use swc_ecma_codegen::{Node, text_writer::JsWriter, Emitter};

mod js_polyfill;

pub fn to_str(swc_node: impl Node) -> String {
    // Emitting the result requires some setup with SWC
    let cm: Arc<SourceMap> = Default::default();
    let mut buff: Vec<u8> = Vec::with_capacity(128);
    let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

    let mut emitter = Emitter {
        cfg: swc_ecma_codegen::Config {
            target: Default::default(),
            ascii_only: false,
            minify: true,
            omit_last_semi: false,
        },
        comments: None,
        wr: writer,
        cm,
    };

    let _ = swc_node.emit_with(&mut emitter);

    String::from_utf8(buff).unwrap()
}

pub fn js(raw: &str) -> Box<Expr> {
    js_polyfill::parse_js(raw).unwrap()
}
