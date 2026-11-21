use fervid_core::{BuiltinType, ElementKind, ElementNode, Node, StartingTag};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{regular_attribute, v_bind_attribute};

#[test]
fn it_generates_empty_teleport() {
    // <teleport></teleport>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "teleport".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Teleport),
        ),
        r#"(_openBlock(),_createBlock(_Teleport))"#,
    )
}

#[test]
fn it_generates_teleport_attrs() {
    // <teleport foo="bar" :baz="qux"></teleport>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "teleport".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Teleport),
        ),
        r#"(_openBlock(),_createBlock(_Teleport,{foo:"bar",baz:qux},null,8,["baz"]))"#,
    )
}

#[test]
fn it_generates_teleport_children() {
    // <teleport>foobar</teleport>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "teleport".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Teleport),
        ),
        r#"(_openBlock(),_createBlock(_Teleport,null,[_createTextVNode("foobar")]))"#,
    )
}

#[test]
fn it_generates_full_teleport() {
    // <teleport foo="bar" :baz="qux">foobar</teleport>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "teleport".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Teleport),
        ),
        r#"(_openBlock(),_createBlock(_Teleport,{foo:"bar",baz:qux},[_createTextVNode("foobar")],8,["baz"]))"#,
    )
}

fn test_out(input: ElementNode, expected: &str) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), false);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
