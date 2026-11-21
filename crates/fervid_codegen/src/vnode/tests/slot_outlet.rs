use fervid_core::{
    AttributeOrBinding, BuiltinType, ElementCodegenValue, ElementKind, ElementNode, Node,
    SfcTemplateBlock, StartingTag, VBindDirective, VCustomDirective, VueDirectives,
};
use fervid_transform::{
    error::{TemplateErrorKind, TransformError},
    template::{
        core::transform_slot_outlet::pre_transform_slot_outlet,
        node_transforms::TransformNodeState, transform_and_record_template,
    },
};
use swc_core::common::{BytePos, DUMMY_SP, Span};

use crate::{
    CodegenContext,
    test_utils::{js, regular_attribute, to_str, transform_context, v_bind_attribute},
};

macro_rules! slot {
    ($attributes: expr, $children: expr) => {
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "slot".into(),
                attributes: $attributes,
                directives: None,
            },
            $children,
            ElementKind::Builtin(BuiltinType::Slot),
        )
    };
    ($attributes: expr) => {
        slot!($attributes, vec![])
    };
}

#[test]
fn it_generates_default_slot() {
    // <slot />
    test_out(slot!(vec![]), r#"_renderSlot(_ctx.$slots,"default")"#);

    // <slot name="default" />
    test_out(
        slot!(vec![regular_attribute("name", "default")]),
        r#"_renderSlot(_ctx.$slots,"default")"#,
    );
}

#[test]
fn it_generates_named_slot() {
    // <slot name="test-slot" />
    test_out(
        slot!(vec![regular_attribute("name", "test-slot")]),
        r#"_renderSlot(_ctx.$slots,"test-slot")"#,
    );
}

#[test]
fn it_generates_dynamically_named_slot() {
    // <slot :name="slot + name" />
    test_out(
        slot!(vec![v_bind_attribute("name", "slot + name")]),
        r#"_renderSlot(_ctx.$slots,_ctx.slot+_ctx.name)"#,
    );
}

#[test]
fn it_generates_attrs() {
    // <slot foo="bar" :baz="qux" />
    test_out(
        slot!(vec![
            regular_attribute("foo", "bar"),
            v_bind_attribute("baz", "qux"),
        ]),
        r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:_ctx.qux})"#,
    );

    // <slot name="default" foo="bar" :baz="qux" />
    test_out(
        slot!(vec![
            regular_attribute("name", "default"),
            regular_attribute("foo", "bar"),
            v_bind_attribute("baz", "qux"),
        ]),
        r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:_ctx.qux})"#,
    );

    // <slot foo="bar" name="default" :baz="qux" />
    test_out(
        slot!(vec![
            regular_attribute("foo", "bar"),
            regular_attribute("name", "default"),
            v_bind_attribute("baz", "qux"),
        ]),
        r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:_ctx.qux})"#,
    );

    // <slot foo="bar" :baz="qux" name="default" />
    test_out(
        slot!(vec![
            regular_attribute("foo", "bar"),
            v_bind_attribute("baz", "qux"),
            regular_attribute("name", "default"),
        ]),
        r#"_renderSlot(_ctx.$slots,"default",{foo:"bar",baz:_ctx.qux})"#,
    );
}

#[test]
fn it_generates_children() {
    // <slot>
    //   <div>Placeholder</div>
    //   <foo-component />
    // </slot>
    test_out(
        slot!(
            vec![],
            vec![
                Node::Element(ElementNode::new_with_children(
                    StartingTag {
                        tag_name: "div".into(),
                        attributes: vec![],
                        directives: None
                    },
                    vec![Node::Text("Placeholder".into(), DUMMY_SP)],
                )),
                Node::Element(ElementNode::new_with_children_and_type(
                    StartingTag {
                        tag_name: "foo-component".into(),
                        attributes: vec![],
                        directives: None
                    },
                    vec![],
                    ElementKind::Component,
                ))
            ]
        ),
        r#"_renderSlot(_ctx.$slots,"default",{},()=>[_createElementVNode("div",null,"Placeholder"),_createVNode(_component_foo_component)])"#,
    );
}

#[test]
fn it_generates_attrs_and_children() {
    // <slot name="test-slot" foo="bar" :baz="qux">
    //   <div>Placeholder</div>
    //   <foo-component />
    // </slot>
    test_out(
        slot!(
            vec![
                regular_attribute("name", "test-slot"),
                regular_attribute("foo", "bar"),
                v_bind_attribute("baz", "qux"),
            ],
            vec![
                Node::Element(ElementNode::new_with_children(
                    StartingTag {
                        tag_name: "div".into(),
                        attributes: vec![],
                        directives: None
                    },
                    vec![Node::Text("Placeholder".into(), DUMMY_SP)],
                )),
                Node::Element(ElementNode::new_with_children_and_type(
                    StartingTag {
                        tag_name: "foo-component".into(),
                        attributes: vec![],
                        directives: None
                    },
                    vec![],
                    ElementKind::Component
                ))
            ]
        ),
        r#"_renderSlot(_ctx.$slots,"test-slot",{foo:"bar",baz:_ctx.qux},()=>[_createElementVNode("div",null,"Placeholder"),_createVNode(_component_foo_component)])"#,
    );
}

#[test]
fn it_normalizes_argumentless_v_bind() {
    // <slot v-bind="obj" />
    test_out(
        slot!(
            vec![AttributeOrBinding::VBind(VBindDirective::new(
                None,
                js("obj"),
            ))],
            vec![]
        ),
        "_renderSlot(_ctx.$slots,\"default\",_normalizeProps(_guardReactiveProps(_ctx.obj)))",
    );
}

#[test]
fn it_merges_argumentless_v_bind_in_source_order() {
    // <slot foo="before" v-bind="obj" bar="after" />
    test_out(
        slot!(
            vec![
                regular_attribute("foo", "before"),
                AttributeOrBinding::VBind(VBindDirective::new(None, js("obj"))),
                regular_attribute("bar", "after"),
            ],
            vec![]
        ),
        "_renderSlot(_ctx.$slots,\"default\",_mergeProps({foo:\"before\"},_ctx.obj,{bar:\"after\"}))",
    );
}

// Vue source: https://github.com/vuejs/core/blob/4ab865a848a1da3d10fb674f857e5fff13094644/packages/compiler-core/__tests__/transforms/transformSlotOutlet.spec.ts

// Duplicated tests skipped

#[test]
fn default_slot_outlet_with_props() {
    // <slot foo="bar" :baz="qux" :foo-bar="foo-bar" />
    test_out(
        slot!(vec![
            regular_attribute("foo", "bar"),
            v_bind_attribute("baz", "qux"),
            v_bind_attribute("foo-bar", "foo-bar"),
        ]),
        "_renderSlot(_ctx.$slots,\"default\",{foo:\"bar\",baz:_ctx.qux,fooBar:_ctx.foo-_ctx.bar})",
    );
}

#[test]
fn statically_named_slot_outlet_with_props() {
    // <slot name="foo" foo="bar" :baz="qux" />
    test_out(
        slot!(vec![
            regular_attribute("name", "foo"),
            regular_attribute("foo", "bar"),
            v_bind_attribute("baz", "qux"),
        ]),
        "_renderSlot(_ctx.$slots,\"foo\",{foo:\"bar\",baz:_ctx.qux})",
    );
}

#[test]
fn dynamically_named_slot_outlet_with_props() {
    // <slot :name="foo" foo="bar" :baz="qux" />
    test_out(
        slot!(vec![
            v_bind_attribute("name", "foo"),
            regular_attribute("foo", "bar"),
            v_bind_attribute("baz", "qux"),
        ]),
        "_renderSlot(_ctx.$slots,_ctx.foo,{foo:\"bar\",baz:_ctx.qux})",
    );
}

#[test]
fn it_ignores_valueless_regular_slot_props() {
    // <slot foo />
    test_out(
        slot!(vec![regular_attribute_empty("foo")]),
        "_renderSlot(_ctx.$slots,\"default\")",
    );
}

#[test]
fn it_preserves_explicitly_empty_regular_slot_props() {
    // <slot foo="" />
    test_out(
        slot!(vec![regular_attribute("foo", "")]),
        "_renderSlot(_ctx.$slots,\"default\",{foo:\"\"})",
    );
}

#[test]
fn it_ignores_valueless_slot_name() {
    // <slot name />
    test_out(
        slot!(vec![regular_attribute_empty("name")]),
        "_renderSlot(_ctx.$slots,\"default\")",
    );
}

#[test]
fn it_preserves_explicitly_empty_slot_name() {
    // <slot name="" />
    test_out(
        slot!(vec![regular_attribute("name", "")]),
        "_renderSlot(_ctx.$slots,\"\")",
    );
}

#[test]
fn default_slot_outlet_with_fallback() {
    // <slot><div /></slot>
    test_out(
        slot!(vec![], vec![element("div")]),
        "_renderSlot(_ctx.$slots,\"default\",{},()=>[_createElementVNode(\"div\")])",
    );
}

#[test]
fn named_slot_outlet_with_fallback() {
    // <slot name="foo"><div /></slot>
    test_out(
        slot!(vec![regular_attribute("name", "foo")], vec![element("div")]),
        "_renderSlot(_ctx.$slots,\"foo\",{},()=>[_createElementVNode(\"div\")])",
    );
}

#[test]
fn default_slot_outlet_with_props_and_fallback() {
    // <slot :foo="bar"><div /></slot>
    test_out(
        slot!(vec![v_bind_attribute("foo", "bar")], vec![element("div")]),
        "_renderSlot(_ctx.$slots,\"default\",{foo:_ctx.bar},()=>[_createElementVNode(\"div\")])",
    );
}

#[test]
fn named_slot_outlet_with_props_and_fallback() {
    // <slot name="foo" :foo="bar"><div /></slot>
    test_out(
        slot!(
            vec![
                regular_attribute("name", "foo"),
                v_bind_attribute("foo", "bar"),
            ],
            vec![element("div")]
        ),
        "_renderSlot(_ctx.$slots,\"foo\",{foo:_ctx.bar},()=>[_createElementVNode(\"div\")])",
    );
}

#[test]
fn slot_with_slotted_false() {
    // <slot />
    // slotted: false
    test_out_with_options(
        slot!(vec![]),
        "_renderSlot(_ctx.$slots,\"default\",{},undefined,true)",
        SlotTestOptions {
            has_scoped_styles: true,
            ..Default::default()
        },
    );
    // <slot>fallback</slot>
    // slotted: false
    test_out_with_options(
        slot!(vec![], vec![Node::Text("fallback".into(), DUMMY_SP)]),
        "_renderSlot(_ctx.$slots,\"default\",{},()=>[_createTextVNode(\"fallback\")],true)",
        SlotTestOptions {
            has_scoped_styles: true,
            ..Default::default()
        },
    );
}

#[test]
fn error_on_unexpected_custom_directive_on_slot() {
    let directive_span = Span::new(BytePos(7), BytePos(12));
    let directives = VueDirectives {
        custom: vec![VCustomDirective {
            name: "foo".into(),
            span: directive_span,
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut element = slot!(vec![]);
    element.starting_tag.directives = Some(Box::new(directives));
    let mut node = Node::Element(element);
    let mut ctx = transform_context();
    let mut state = TransformNodeState::default();

    // <slot v-foo />
    pre_transform_slot_outlet(&mut ctx, &mut state, &mut node);

    assert!(matches!(
        ctx.errors.as_slice(),
        [TransformError::TemplateError(error)]
            if matches!(error.kind, TemplateErrorKind::VSlotUnexpectedDirectiveOnSlotOutlet)
                && error.span == directive_span
    ));
}

#[test]
fn dynamically_named_slot_outlet_with_v_bind_shorthand() {
    // <slot :name />
    test_out(
        slot!(vec![v_bind_attribute("name", "name")]),
        "_renderSlot(_ctx.$slots,_ctx.name)",
    );
}

#[test]
fn it_keeps_slotted_scope_for_scoped_styles_with_slotted_usage() {
    // <slot />
    test_out_with_options(
        slot!(vec![]),
        "_renderSlot(_ctx.$slots,\"default\")",
        SlotTestOptions {
            has_scoped_styles: true,
            slotted: true,
            ..Default::default()
        },
    );
}

#[test]
fn it_preserves_props_position_when_disabling_slotted_scope() {
    // <slot foo="bar" />
    // slotted: false
    test_out_with_options(
        slot!(vec![regular_attribute("foo", "bar")]),
        "_renderSlot(_ctx.$slots,\"default\",{foo:\"bar\"},undefined,true)",
        SlotTestOptions {
            has_scoped_styles: true,
            ..Default::default()
        },
    );
}

#[test]
fn it_generates_text_fallback_as_an_array() {
    // <slot>fallback</slot>
    test_out(
        slot!(vec![], vec![Node::Text("fallback".into(), DUMMY_SP)]),
        "_renderSlot(_ctx.$slots,\"default\",{},()=>[_createTextVNode(\"fallback\")])",
    );
}

// Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vFor.spec.ts#L921-L969
#[test]
fn template_v_for_key_injection_with_single_slot_child() {
    // <template v-for="item in items" :key="item.id"><slot /></template>
    // slotted: false
    test_out_with_options(
        slot!(vec![]),
        "_renderSlot(_ctx.$slots,\"default\",{},undefined,undefined,item.id)",
        SlotTestOptions {
            branch_key: Some("item.id"),
            ..Default::default()
        },
    );
}

#[test]
fn template_v_for_key_injection_with_slot_fallback() {
    // <template v-for="item in items" :key="item.id"><slot>fallback</slot></template>
    // slotted: false
    test_out_with_options(
        slot!(vec![], vec![Node::Text("fallback".into(), DUMMY_SP)]),
        "_renderSlot(_ctx.$slots,\"default\",{},()=>[_createTextVNode(\"fallback\")],undefined,item.id)",
        SlotTestOptions {
            branch_key: Some("item.id"),
            ..Default::default()
        },
    );
}

#[test]
fn template_v_for_key_injection_with_slot_props() {
    // <template v-for="item in items" :key="item.id"><slot foo="bar" /></template>
    // slotted: false
    test_out_with_options(
        slot!(vec![regular_attribute("foo", "bar")]),
        "_renderSlot(_ctx.$slots,\"default\",{foo:\"bar\"},undefined,undefined,item.id)",
        SlotTestOptions {
            branch_key: Some("item.id"),
            ..Default::default()
        },
    );
}

#[test]
fn template_v_for_key_injection_with_no_slotted_scope() {
    // <template v-for="item in items" :key="item.id"><slot /></template>
    // slotted: false
    test_out_with_options(
        slot!(vec![]),
        "_renderSlot(_ctx.$slots,\"default\",{},undefined,true,item.id)",
        SlotTestOptions {
            has_scoped_styles: true,
            branch_key: Some("item.id"),
            ..Default::default()
        },
    );
}

#[test]
fn conditional_slot_outlet_preserves_user_key() {
    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/src/utils.ts#L480-L507
    // <slot v-if="ok" :key="userKey" />
    let mut input = slot!(vec![v_bind_attribute("key", "userKey")]);
    input.starting_tag.directives = Some(Box::new(VueDirectives {
        v_if: Some(js("ok")),
        ..Default::default()
    }));
    test_out(
        input,
        "_ctx.ok?_renderSlot(_ctx.$slots,\"default\",{key:_ctx.userKey}):_createCommentVNode(\"v-if\",true)",
    );
}

#[test]
fn conditional_slot_outlet_receives_sixth_branch_key() {
    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L525-L543
    // <slot v-if="ok" />
    let mut input = slot!(vec![]);
    input.starting_tag.directives = Some(Box::new(VueDirectives {
        v_if: Some(js("ok")),
        ..Default::default()
    }));
    test_out(
        input,
        "_ctx.ok?_renderSlot(_ctx.$slots,\"default\",{},undefined,undefined,0):_createCommentVNode(\"v-if\",true)",
    );
}

fn test_out(input: ElementNode, expected: &str) {
    test_out_with_options(input, expected, SlotTestOptions::default());
}

#[derive(Default)]
struct SlotTestOptions<'a> {
    has_scoped_styles: bool,
    slotted: bool,
    // Already processed expression, matching the key supplied by the structural transform
    branch_key: Option<&'a str>,
}

fn test_out_with_options(input: ElementNode, expected: &str, options: SlotTestOptions) {
    let mut template = SfcTemplateBlock {
        lang: "html".into(),
        roots: vec![Node::Element(input)],
        span: DUMMY_SP,
    };
    let mut ctx = transform_context();
    ctx.has_scoped_styles = options.has_scoped_styles;
    ctx.slotted = options.slotted;
    transform_and_record_template(&mut template, &mut ctx);
    assert!(ctx.errors.is_empty(), "{:#?}", ctx.errors);

    assert_eq!(
        template.roots.len(),
        1,
        "slot fixture needs exactly one root"
    );
    let mut node = template.roots.pop().expect("slot fixture needs a root");
    if let Some(branch_key) = options.branch_key {
        let Node::Element(element) = &mut node else {
            panic!("slot outlet must be an element");
        };
        let Some(codegen_node) = element.codegen_node.as_mut() else {
            panic!("slot outlet must have a codegen node");
        };
        let ElementCodegenValue::SlotOutletCall(slot_call) = &mut codegen_node.value else {
            panic!("slot outlet must generate renderSlot");
        };
        slot_call.branch_key = Some(js(branch_key));
    }

    let mut codegen = CodegenContext::with_bindings_helper(ctx.bindings_helper);
    assert_eq!(to_str(codegen.generate_node(&node, false)), expected);
}

fn element(tag_name: &str) -> Node {
    Node::Element(ElementNode::new_with_children_and_type(
        StartingTag {
            tag_name: tag_name.into(),
            attributes: vec![],
            directives: None,
        },
        vec![],
        ElementKind::Element,
    ))
}

fn regular_attribute_empty(name: &str) -> AttributeOrBinding {
    AttributeOrBinding::RegularAttribute {
        name: name.into(),
        value: None,
        span: DUMMY_SP,
    }
}
