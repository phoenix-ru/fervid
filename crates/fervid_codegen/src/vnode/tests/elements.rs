use fervid_core::{
    AttributeOrBinding, ElementKind, ElementNode, Interpolation, Node, StartingTag, VBindDirective,
};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{js, regular_attribute, v_bind_attribute, v_on_attribute};

#[test]
fn it_generates_basic_usage() {
    // <div
    //   foo="bar"
    //   :baz="qux"
    //   :readonly="true"
    //   @click="handleClick"
    // >hello from div</div>

    // Mark `readonly` prop as CanStringify so it is skipped from dynamic prop list
    let mut readonly_true = VBindDirective::new(Some("readonly".into()), Box::new(true.into()));
    readonly_true.const_type = fervid_core::ConstantTypes::CanStringify;

    test_out(
        ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                    AttributeOrBinding::VBind(readonly_true),
                    // Current directive transform takes the source event name
                    v_on_attribute("click", "handleClick"),
                ],
                directives: None,
            },
            children: vec![Node::Text("hello from div".into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        },
        r#"_createElementVNode("div",{foo:"bar",baz:qux,readonly:true,onClick:_ctx.handleClick},"hello from div",8,["baz","onClick"])"#,
        false,
    )
}

#[test]
fn it_generates_attrless() {
    // <div>hello from div</div>
    test_out(
        ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![],
                directives: None,
            },
            children: vec![Node::Text("hello from div".into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        },
        r#"_createElementVNode("div",null,"hello from div")"#,
        false,
    )
}

#[test]
fn it_generates_childless() {
    // <div foo="bar"></div>
    test_out(
        ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("some-baz", "qux"),
                ],
                directives: None,
            },
            children: vec![],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        },
        r#"_createElementVNode("div",{foo:"bar","some-baz":qux},null,8,["some-baz"])"#,
        false,
    );

    // <div foo="bar" />
    test_out(
        ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("some-baz", "qux"),
                ],
                directives: None,
            },
            children: vec![],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        },
        r#"_createElementVNode("div",{foo:"bar","some-baz":qux},null,8,["some-baz"])"#,
        false,
    )
}

#[test]
fn it_generates_text_nodes_concatenation() {
    // <div>hello from div {{ true }} bye!</div>
    test_out(
        ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![],
                directives: None,
            },
            children: vec![
                Node::Text("hello from div ".into(), DUMMY_SP),
                Node::Interpolation(Interpolation {
                    value: js("true"),
                    template_scope: 0,
                    patch_flag: false,
                    span: DUMMY_SP,
                }),
                Node::Text(" bye!".into(), DUMMY_SP),
            ],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        },
        r#"_createElementVNode("div",null,"hello from div "+_toDisplayString(true)+" bye!")"#,
        false,
    )
}

#[test]
fn it_generates_children() {
    // <div>hello from div {{ true }}<span>bye!</span></div>
    test_out(
        ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![],
                directives: None,
            },
            children: vec![
                Node::Text("hello from div ".into(), DUMMY_SP),
                Node::Interpolation(Interpolation {
                    value: js("true"),
                    template_scope: 0,
                    patch_flag: false,
                    span: DUMMY_SP,
                }),
                Node::Element(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "span".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    children: vec![Node::Text("bye!".into(), DUMMY_SP)],
                    template_scope: 0,
                    tag_type: ElementKind::Element,
                    patch_hints: Default::default(),
                    span: DUMMY_SP,
                    codegen_node: None,
                }),
            ],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        },
        r#"_createElementVNode("div",null,[_createTextVNode("hello from div "+_toDisplayString(true)),_createElementVNode("span",null,"bye!")])"#,
        false,
    )
}

fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), wrap_in_block);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
