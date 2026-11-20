use fervid_core::{BuiltinType, ElementKind, ElementNode, Node, StartingTag};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{regular_attribute, v_bind_attribute};

#[test]
fn it_generates_empty_transition_group() {
    // <transition-group></transition-group>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition-group".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::TransitionGroup),
        ),
        r#"_createVNode(_TransitionGroup)"#,
    )
}

#[test]
fn it_generates_transition_group_attrs() {
    // <transition-group foo="bar" :baz="qux"></transition-group>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition-group".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::TransitionGroup),
        ),
        r#"_createVNode(_TransitionGroup,{foo:"bar",baz:qux},null,8,["baz"])"#,
    )
}

#[test]
fn it_generates_transition_group_children() {
    // <transition-group>foobar</transition-group>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition-group".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::TransitionGroup),
        ),
        r#"_createVNode(_TransitionGroup,null,{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1})"#,
    )
}

#[test]
fn it_generates_full_transition_group() {
    // <transition-group foo="bar" :baz="qux">foobar</transition-group>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "transition-group".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::TransitionGroup),
        ),
        r#"_createVNode(_TransitionGroup,{foo:"bar",baz:qux},{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1},8,["baz"])"#,
    )
}

fn test_out(input: ElementNode, expected: &str) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), false);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
