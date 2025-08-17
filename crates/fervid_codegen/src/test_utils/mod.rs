use fervid_core::{AttributeOrBinding, VBindDirective, VOnDirective};
use swc_core::{
    common::{SourceMap, DUMMY_SP},
    ecma::ast::Expr,
};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};

mod js_polyfill;

pub fn to_str(swc_node: impl Node) -> String {
    // Emitting the result requires some setup with SWC
    let cm: swc_core::common::sync::Lrc<SourceMap> = Default::default();
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

    String::from_utf8(buff).expect("buff must be a valid utf8 string")
}

pub fn js(raw: &str) -> Box<Expr> {
    js_polyfill::parse_js(raw).expect("input must be a valid js")
}

/// TEST ONLY
#[inline]
pub fn regular_attribute(name: &str, value: &str) -> AttributeOrBinding {
    AttributeOrBinding::RegularAttribute {
        name: name.into(),
        value: value.into(),
        span: DUMMY_SP,
    }
}

/// TEST ONLY
#[inline]
pub fn v_bind_attribute(name: &str, value: &str) -> AttributeOrBinding {
    AttributeOrBinding::VBind(VBindDirective {
        argument: Some(name.into()),
        value: js(value),
        is_camel: false,
        is_prop: false,
        is_attr: false,
        span: DUMMY_SP,
    })
}

/// TEST ONLY
#[inline]
pub fn v_on_attribute(name: &str, value: &str) -> AttributeOrBinding {
    AttributeOrBinding::VOn(VOnDirective {
        event: Some(name.into()),
        handler: Some(js(value)),
        modifiers: vec![],
        span: DUMMY_SP,
    })
}
