use fervid_core::{CacheExpression, CompoundExpressionNode, ExpressionNode, JsChildNode};

use fervid_core::{
    ElementKind, ElementNode, ForParseResult, Interpolation, Node, PatchHints, SfcDescriptor,
    SfcTemplateBlock, StartingTag, StrOrExpr, VForDirective, VSlotDirective, VueDirectives,
};
use swc_core::common::DUMMY_SP;

use crate::CodegenContext;
use crate::test_utils::{js, pat, to_str};

#[test]
fn it_generates_cache_expressions() {
    let cache = CacheExpression {
        value: JsChildNode::ExpressionNode(Box::new(ExpressionNode::CompoundExpression(
            CompoundExpressionNode {
                ast: js("(...args) => handler(...args)"),
                is_handler_key: false,
            },
        ))),
        markers: Default::default(),
    };
    let mut ctx = CodegenContext::default();

    assert_eq!(
        to_str(ctx.generate_cache_expression(&cache)),
        "_cache[0]||(_cache[0]=(...args)=>handler(...args))"
    );
    assert_eq!(
        to_str(ctx.generate_cache_expression(&cache)),
        "_cache[1]||(_cache[1]=(...args)=>handler(...args))"
    );
}

#[test]
fn it_generates_static_slots() {
    assert_eq!(
        // <template v-slot:header>static</template>
        transform_and_generate_slots(vec![slot("header", "static", None)]),
        "(_openBlock(),_createBlock(_component_Comp,null,{header:_withCtx(()=>[_createTextVNode(\"static\")]),_:1}))"
    );
}

#[test]
fn it_generates_dynamic_slots() {
    assert_eq!(
        // <template v-if="ok" v-slot:header>dynamic</template>
        transform_and_generate_slots(vec![slot("header", "dynamic", Some("ok"))]),
        "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({_:2},[_ctx.ok?{name:\"header\",fn:_withCtx(()=>[_createTextVNode(\"dynamic\")]),key:0}:undefined]),1024))"
    );
}

#[test]
fn it_generates_static_and_dynamic_slots() {
    assert_eq!(
        transform_and_generate_slots(vec![
            // <template v-slot:header>static</template>
            slot("header", "static", None),
            // <template v-if="ok" v-slot:footer>dynamic</template>
            slot("footer", "dynamic", Some("ok")),
        ]),
        "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({header:_withCtx(()=>[_createTextVNode(\"static\")]),_:2},[_ctx.ok?{name:\"footer\",fn:_withCtx(()=>[_createTextVNode(\"dynamic\")]),key:0}:undefined]),1024))"
    );
}

#[test]
fn it_generates_conditional_slot_chain() {
    assert_eq!(
        transform_and_generate_slots(vec![
            // <template v-if="ok" v-slot:one>one</template>
            slot_with_condition("one", "one", Some("ok"), None, false),
            // <template v-else-if="other" v-slot:two>two</template>
            slot_with_condition("two", "two", None, Some("other"), false),
            // <template v-else v-slot:three>three</template>
            slot_with_condition("three", "three", None, None, true),
        ]),
        "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({_:2},[_ctx.ok?{name:\"one\",fn:_withCtx(()=>[_createTextVNode(\"one\")]),key:0}:_ctx.other?{name:\"two\",fn:_withCtx(()=>[_createTextVNode(\"two\")]),key:1}:{name:\"three\",fn:_withCtx(()=>[_createTextVNode(\"three\")]),key:2}]),1024))"
    );
}

#[test]
fn it_generates_v_for_dynamic_slots() {
    assert_eq!(
        // <template v-for="item in items" v-slot:[item.name]>{{ item.value }}</template>
        transform_and_generate_slots(vec![v_for_slot()]),
        "(_openBlock(),_createBlock(_component_Comp,null,_createSlots({_:2},[_renderList(_ctx.items,item=>({name:item.name,fn:_withCtx(()=>[_createTextVNode(_toDisplayString(item.value),1)])}))]),1024))"
    );
}

fn transform_and_generate_slots(children: Vec<Node>) -> String {
    let mut template = SfcTemplateBlock {
        lang: "html".into(),
        roots: vec![Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "Comp".into(),
                attributes: vec![],
                directives: None,
            },
            children,
            template_scope: 0,
            tag_type: ElementKind::Component,
            patch_hints: PatchHints::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })],
        span: DUMMY_SP,
    };
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
    let mut transform_ctx = fervid_transform::TransformSfcContext::new(&descriptor, &options);
    fervid_transform::template::transform_and_record_template(&mut template, &mut transform_ctx);
    assert!(transform_ctx.errors.is_empty());

    let mut codegen_ctx = CodegenContext::default();
    to_str(codegen_ctx.generate_node(&template.roots[0], true))
}

fn slot(name: &str, text: &str, condition: Option<&str>) -> Node {
    slot_with_condition(name, text, condition, None, false)
}

fn slot_with_condition(
    name: &str,
    text: &str,
    v_if: Option<&str>,
    v_else_if: Option<&str>,
    v_else: bool,
) -> Node {
    Node::Element(ElementNode {
        starting_tag: StartingTag {
            tag_name: "template".into(),
            attributes: vec![],
            directives: Some(Box::new(VueDirectives {
                v_else: v_else.then_some(()),
                v_else_if: v_else_if.map(js),
                v_if: v_if.map(js),
                v_slot: Some(VSlotDirective {
                    slot_name: Some(name.into()),
                    value: None,
                }),
                ..Default::default()
            })),
        },
        children: vec![Node::Text(text.into(), DUMMY_SP)],
        template_scope: 0,
        tag_type: ElementKind::Template,
        patch_hints: PatchHints::default(),
        span: DUMMY_SP,
        codegen_node: None,
    })
}

fn v_for_slot() -> Node {
    Node::Element(ElementNode {
        starting_tag: StartingTag {
            tag_name: "template".into(),
            attributes: vec![],
            directives: Some(Box::new(VueDirectives {
                v_for: Some(VForDirective {
                    parse_result: Box::new(ForParseResult {
                        source: js("items"),
                        value: pat("item"),
                        key: None,
                        index: None,
                        finalized: false,
                        finalized_is_dynamic: false,
                        source_const_type: Default::default(),
                    }),
                    patch_flags: Default::default(),
                    span: DUMMY_SP,
                }),
                v_slot: Some(VSlotDirective {
                    slot_name: Some(StrOrExpr::Expr(js("item.name"))),
                    value: None,
                }),
                ..Default::default()
            })),
        },
        children: vec![Node::Interpolation(Interpolation {
            value: js("item.value"),
            template_scope: 0,
            patch_flag: false,
            span: DUMMY_SP,
        })],
        template_scope: 0,
        tag_type: ElementKind::Template,
        patch_hints: PatchHints::default(),
        span: DUMMY_SP,
        codegen_node: None,
    })
}
