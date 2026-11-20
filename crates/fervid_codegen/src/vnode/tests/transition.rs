use fervid_core::{BuiltinType, ElementKind, ElementNode, Node, StartingTag};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{regular_attribute, v_bind_attribute};

#[test]
fn it_generates_empty_transition() {
    // <transition></transition>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Transition),
        ),
        r#"_createVNode(_Transition)"#,
    )
}

#[test]
fn it_generates_transition_attrs() {
    // <transition foo="bar" :baz="qux"></transition>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Transition),
        ),
        r#"_createVNode(_Transition,{foo:"bar",baz:qux},null,8,["baz"])"#,
    )
}

#[test]
fn it_generates_transition_children() {
    // <transition>foobar</transition>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Transition),
        ),
        r#"_createVNode(_Transition,null,{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1})"#,
    )
}

#[test]
fn it_generates_full_transition() {
    // <transition foo="bar" :baz="qux">foobar</transition>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Transition),
        ),
        r#"_createVNode(_Transition,{foo:"bar",baz:qux},{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1},8,["baz"])"#,
    )
}

fn test_out(input: ElementNode, expected: &str) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), false);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
