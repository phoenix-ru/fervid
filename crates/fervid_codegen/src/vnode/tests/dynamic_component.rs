use fervid_core::{
    BuiltinType, ElementKind, ElementNode, Node, StartingTag, VSlotDirective, VueDirectives,
};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{regular_attribute, template_carrier, v_bind_attribute};

#[test]
fn it_generates_component_without_is() {
    // <component></component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        "_createVNode(_component_component)",
    );
}

#[test]
fn it_generates_component_is_static() {
    // <component is="div"></component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![regular_attribute("is", "div")],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div")))"#,
    );
}

#[test]
fn it_generates_component_is_binding() {
    // <component :is="foo"></component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![v_bind_attribute("is", "foo")],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        r#"(_openBlock(),_createBlock(_resolveDynamicComponent(foo)))"#,
    );
}

#[test]
fn it_generates_component_builtin_attrs() {
    // <component is="div" foo="bar" :baz="qux"></component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![
                    regular_attribute("is", "div"),
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),{foo:"bar",baz:qux},null,8,["baz"]))"#,
    )
}

#[test]
fn it_generates_component_builtin_default_slot() {
    // <component is="div">foobar</component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![regular_attribute("is", "div")],
                directives: None,
            },
            vec![Node::Text("foobar".into(), DUMMY_SP)],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),null,{default:_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
    )
}

#[test]
fn it_generates_component_builtin_named_slot() {
    // <component is="div">
    //   <template v-slot:named>foobar</template>
    // </component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![regular_attribute("is", "div")],
                directives: None,
            },
            vec![Node::Element(template_carrier(
                StartingTag {
                    tag_name: "template".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_slot: Some(VSlotDirective {
                            slot_name: Some("named".into()),
                            value: None,
                        }),
                        ..Default::default()
                    })),
                },
                vec![Node::Text("foobar".into(), DUMMY_SP)],
            ))],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),null,{named:_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
    )
}

#[test]
fn it_generates_full_component_builtin() {
    // <component is="div" foo="bar" :baz="qux">
    //   foobar
    //   <template v-slot:named>
    //     bazqux
    //   </template>
    // </component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "component".into(),
                attributes: vec![
                    regular_attribute("is", "div"),
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("baz", "qux"),
                ],
                directives: None,
            },
            vec![
                Node::Text("foobar".into(), DUMMY_SP),
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("named".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![Node::Text("bazqux".into(), DUMMY_SP)],
                )),
            ],
            ElementKind::Builtin(BuiltinType::Component),
        ),
        r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),{foo:"bar",baz:qux},{named:_withCtx(()=>[_createTextVNode("bazqux")]),default:_withCtx(()=>[_createTextVNode("foobar")]),_:1},8,["baz"]))"#,
    )
}

fn test_out(input: ElementNode, expected: &str) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), false);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
