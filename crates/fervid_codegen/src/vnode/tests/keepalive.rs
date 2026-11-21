use fervid_core::{BuiltinType, ElementKind, ElementNode, Node, StartingTag};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{regular_attribute, v_bind_attribute};

#[test]
fn it_generates_empty_keepalive() {
    // <keep-alive></keep-alive>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "keep-alive".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::KeepAlive),
        ),
        r#"_createVNode(_KeepAlive)"#,
    )
}

#[test]
fn it_generates_keepalive_attrs() {
    // <keep-alive foo="bar" :baz="qux"></keep-alive>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "keep-alive".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::KeepAlive),
        ),
        r#"_createVNode(_KeepAlive,{foo:"bar",baz:qux},null,8,["baz"])"#,
    )
}

#[test]
fn it_generates_keepalive_children() {
    // <keep-alive>foobar</keep-alive>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "keep-alive".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::KeepAlive),
        ),
        r#"(_openBlock(),_createBlock(_KeepAlive,null,[_createTextVNode("foobar")],1024))"#,
    )
}

#[test]
fn it_generates_full_keepalive() {
    // <keep-alive foo="bar" :baz="qux">foobar</keep-alive>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "keep-alive".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::KeepAlive),
        ),
        r#"(_openBlock(),_createBlock(_KeepAlive,{foo:"bar",baz:qux},[_createTextVNode("foobar")],1032,["baz"]))"#,
    )
}

fn test_out(input: ElementNode, expected: &str) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), false);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
