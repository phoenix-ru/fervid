pub mod parser;

use fervid_core::{ElementNode, Node, StartingTag, fervid_atom};
use parser::parse_typescript_module;
use swc_core::common::{SourceMap, sync::Lrc};
use swc_core::ecma::ast::{Expr, Module};
use swc_ecma_codegen::{Emitter, Node as CodegenNode, text_writer::JsWriter};

use self::parser::{parse_javascript_expr, parse_typescript_expr};

pub fn js(raw: &str) -> Box<Expr> {
    parse_javascript_expr(raw, 0, Default::default()).unwrap().0
}

pub fn ts(raw: &str) -> Box<Expr> {
    parse_typescript_expr(raw, 0, Default::default()).unwrap().0
}

pub fn ts_module(raw: &str) -> Module {
    parse_typescript_module(raw, 0, Default::default())
        .expect("Expected input to be parseable")
        .0
}

pub fn element_from_tag(tag_name: &str) -> ElementNode {
    ElementNode::new(StartingTag {
        tag_name: tag_name.into(),
        attributes: vec![],
        directives: None,
    })
}

pub fn element_with_children(children: Vec<Node>) -> ElementNode {
    ElementNode::new_with_children(
        StartingTag {
            tag_name: fervid_atom!("div"),
            attributes: vec![],
            directives: None,
        },
        children,
    )
}

pub fn to_str(swc_node: &impl CodegenNode) -> String {
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
