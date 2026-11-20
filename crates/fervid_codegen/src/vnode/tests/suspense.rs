use fervid_core::{BuiltinType, ElementKind, ElementNode, Node, StartingTag};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{regular_attribute, v_bind_attribute};

#[test]
fn it_generates_empty_suspense() {
    // <suspense></suspense>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "suspense".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Suspense),
        ),
        r#"(_openBlock(),_createBlock(_Suspense))"#,
    )
}

#[test]
fn it_generates_suspense_attrs() {
    // <suspense foo="bar" :baz="qux"></suspense>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "suspense".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Suspense),
        ),
        r#"(_openBlock(),_createBlock(_Suspense,{foo:"bar",baz:qux},null,8,["baz"]))"#,
    )
}

#[test]
fn it_generates_suspense_children() {
    // <suspense>foobar</suspense>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "suspense".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Suspense),
        ),
        r#"(_openBlock(),_createBlock(_Suspense,null,{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
    )
}

#[test]
fn it_generates_full_suspense() {
    // <suspense foo="bar" :baz="qux">foobar</suspense>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "suspense".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Suspense),
        ),
        r#"(_openBlock(),_createBlock(_Suspense,{foo:"bar",baz:qux},{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1},8,["baz"]))"#,
    )
}

fn test_out(input: ElementNode, expected: &str) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), false);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
