use fervid_core::{
    AttributeOrBinding, ElementKind, ElementNode, Node as TemplateNode, SfcDescriptor, StartingTag,
    VBindDirective, VOnDirective,
};
use fervid_transform::{
    TransformSfcContext,
    template::{
        core::transform_slot_outlet::pre_transform_slot_outlet,
        node_transforms::TransformNodeState,
        transform_element::{build_props, post_transform_element_node},
    },
};
use swc_core::{
    common::{DUMMY_SP, SourceMap},
    ecma::ast::{Expr, Pat},
};
use swc_ecma_codegen::{Emitter, Node, text_writer::JsWriter};

use crate::CodegenContext;

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
    js_polyfill::parse_js_into_expr(raw).expect("input must be a valid js")
}

pub fn pat(raw: &str) -> Box<Pat> {
    Box::new(js_polyfill::parse_js_into_pat(raw).expect("input must be a valid binding pattern"))
}

/// TEST ONLY
#[inline]
pub fn regular_attribute(name: &str, value: &str) -> AttributeOrBinding {
    AttributeOrBinding::RegularAttribute {
        name: name.into(),
        value: Some(value.into()),
        span: DUMMY_SP,
    }
}

/// TEST ONLY
#[inline]
pub fn v_bind_attribute(name: &str, value: &str) -> AttributeOrBinding {
    AttributeOrBinding::VBind(VBindDirective::new(Some(name.into()), js(value)))
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

/// Create a new ElementNode with ElementKind::Template
pub fn template_carrier(starting_tag: StartingTag, children: Vec<TemplateNode>) -> ElementNode {
    ElementNode::new_with_children_and_type(starting_tag, children, ElementKind::Template)
}

// Do the transform phase before testing code generation
pub fn prepare_codegen(node: &mut TemplateNode, ctx: &mut TransformSfcContext) {
    match node {
        TemplateNode::Element(element) => {
            for child in &mut element.children {
                prepare_codegen(child, ctx);
            }
            if element.codegen_node.is_some() {
                return;
            }
            let mut state = TransformNodeState {
                current_scope: element.template_scope,
                ..Default::default()
            };
            pre_transform_slot_outlet(ctx, &mut state, node);
            post_transform_element_node(node, ctx, &mut state);
        }
        TemplateNode::For(for_node) => {
            for child in &mut for_node.children {
                prepare_codegen(child, ctx);
            }
        }
        TemplateNode::ConditionalSeq(sequence) => {
            prepare_codegen(&mut sequence.if_node.node, ctx);
            for branch in &mut sequence.else_if_nodes {
                prepare_codegen(&mut branch.node, ctx);
            }
            if let Some(branch) = &mut sequence.else_node {
                prepare_codegen(branch, ctx);
            }
        }
        TemplateNode::Text(..) | TemplateNode::Interpolation(_) | TemplateNode::Comment(..) => {}
    }
}

pub fn generate_prepared_node(mut node: TemplateNode, wrap_in_block: bool) -> Expr {
    let mut ctx = transform_context();
    prepare_codegen(&mut node, &mut ctx);
    CodegenContext::with_bindings_helper(ctx.bindings_helper).generate_node(&node, wrap_in_block)
}

pub fn generate_prepared_props(mut node: ElementNode) -> Expr {
    let mut ctx = transform_context();
    let mut state = TransformNodeState::default();
    let is_component = matches!(node.tag_type, ElementKind::Component);
    let result = build_props(
        &mut node,
        &mut ctx,
        &mut state,
        &[],
        is_component,
        false,
        false,
    );
    let mut codegen = CodegenContext::with_bindings_helper(ctx.bindings_helper);
    result.props.map_or_else(
        || {
            Expr::Object(swc_core::ecma::ast::ObjectLit {
                span: DUMMY_SP,
                props: vec![],
            })
        },
        |props| codegen.generate_props_expression(&props),
    )
}

pub fn transform_context() -> TransformSfcContext {
    let descriptor = SfcDescriptor::default();
    let options = fervid_transform::TransformSfcOptions {
        is_prod: false,
        is_ce: false,
        props_destructure: Default::default(),
        scope_id: "",
        filename: "anonymous.vue",
        transform_asset_urls: Default::default(),
        directive_transforms: Default::default(),
        node_transforms: Default::default(),
    };
    let mut ctx = TransformSfcContext::new(&descriptor, &options);
    ctx.cache_handlers = false;
    ctx
}
