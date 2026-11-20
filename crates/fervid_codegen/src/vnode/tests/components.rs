use fervid_core::{
    ElementKind, ElementNode, Interpolation, Node, StartingTag, VSlotDirective, VueDirectives,
};
use swc_core::common::DUMMY_SP;

use crate::test_utils::{js, regular_attribute, template_carrier, v_bind_attribute};

#[test]
fn it_generates_basic_usage() {
    // <test-component></test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Component,
        ),
        r"_createVNode(_component_test_component)",
        false,
    );

    // <test-component />
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Component,
        ),
        r"_createVNode(_component_test_component)",
        false,
    );
}

#[test]
fn it_generates_attributes() {
    // <test-component foo="bar" :baz="qux"></test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![
                    regular_attribute("foo", "bar"),
                    v_bind_attribute("some-baz", "qux"),
                ],
                directives: None,
            },
            vec![],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,{foo:"bar","some-baz":qux},null,8,["some-baz"])"#,
        false,
    );
}

#[test]
fn it_generates_default_slot() {
    // <test-component>hello from component<div>hello from div</div></test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![
                Node::Text("hello from component".into(), DUMMY_SP),
                Node::Element(ElementNode::new_with_children(
                    StartingTag {
                        tag_name: "div".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    vec![Node::Text("hello from div".into(), DUMMY_SP)],
                )),
            ],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{default:_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")]),_:1})"#,
        false,
    );

    // <test-component>
    //   <template v-slot:default>hello from component<div>hello from div</div></template>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Element(template_carrier(
                StartingTag {
                    tag_name: "template".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_slot: Some(VSlotDirective {
                            slot_name: Some("default".into()),
                            value: None,
                        }),
                        ..Default::default()
                    })),
                },
                vec![
                    Node::Text("hello from component".into(), DUMMY_SP),
                    Node::Element(ElementNode::new_with_children(
                        StartingTag {
                            tag_name: "div".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        vec![Node::Text("hello from div".into(), DUMMY_SP)],
                    )),
                ],
            ))],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{default:_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")]),_:1})"#,
        false,
    );
}

#[test]
fn it_generates_named_slot() {
    // <test-component>
    //   <template v-slot:foo-bar>hello from component<div>hello from div</div></template>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![Node::Element(template_carrier(
                StartingTag {
                    tag_name: "template".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_slot: Some(VSlotDirective {
                            slot_name: Some("foo-bar".into()),
                            value: None,
                        }),
                        ..Default::default()
                    })),
                },
                vec![
                    Node::Text("hello from component".into(), DUMMY_SP),
                    Node::Element(ElementNode::new_with_children(
                        StartingTag {
                            tag_name: "div".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        vec![Node::Text("hello from div".into(), DUMMY_SP)],
                    )),
                ],
            ))],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")]),_:1})"#,
        false,
    );
}

#[test]
fn it_generates_multiple_named_slots() {
    // <test-component>
    //   <template v-slot:foo-bar>hello from slot {{ one }}</template>
    //   <template v-slot:baz>hello from slot <b>two</b></template>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("foo-bar".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![
                        Node::Text("hello from slot ".into(), DUMMY_SP),
                        Node::Interpolation(Interpolation {
                            value: js("one"),
                            template_scope: 0,
                            patch_flag: true,
                            span: DUMMY_SP,
                        }),
                    ],
                )),
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("baz".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![
                        Node::Text("hello from slot ".into(), DUMMY_SP),
                        Node::Element(ElementNode::new_with_children(
                            StartingTag {
                                tag_name: "b".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            vec![Node::Text("two".into(), DUMMY_SP)],
                        )),
                    ],
                )),
            ],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot "+_toDisplayString(one),1)]),baz:_withCtx(()=>[_createTextVNode("hello from slot "),_createElementVNode("b",null,"two")]),_:1})"#,
        false,
    );
}

#[test]
fn it_generates_mixed_slots() {
    // <test-component>
    //   hello from component<div>hello from div</div>
    //   <template v-slot:foo-bar>hello from slot</template>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![
                Node::Text("hello from component".into(), DUMMY_SP),
                Node::Element(ElementNode::new_with_children(
                    StartingTag {
                        tag_name: "div".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    vec![Node::Text("hello from div".into(), DUMMY_SP)],
                )),
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("foo-bar".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![Node::Text("hello from slot".into(), DUMMY_SP)],
                )),
            ],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),default:_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")]),_:1})"#,
        false,
    );

    // <test-component>
    //   <template v-slot>hello from default<div>hello from div</div></template>
    //   <template v-slot:foo-bar>hello from slot</template>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: None,
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![
                        Node::Text("hello from default".into(), DUMMY_SP),
                        Node::Element(ElementNode::new_with_children(
                            StartingTag {
                                tag_name: "div".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            vec![Node::Text("hello from div".into(), DUMMY_SP)],
                        )),
                    ],
                )),
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("foo-bar".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![Node::Text("hello from slot".into(), DUMMY_SP)],
                )),
            ],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{default:_withCtx(()=>[_createTextVNode("hello from default"),_createElementVNode("div",null,"hello from div")]),"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),_:1})"#,
        false,
    );

    // <test-component>
    //   <template v-slot:foo-bar>hello from slot</template>
    //   hello from component<div>hello from div</div>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("foo-bar".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![Node::Text("hello from slot".into(), DUMMY_SP)],
                )),
                Node::Text("hello from component".into(), DUMMY_SP),
                Node::Element(ElementNode::new_with_children(
                    StartingTag {
                        tag_name: "div".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    vec![Node::Text("hello from div".into(), DUMMY_SP)],
                )),
            ],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),default:_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")]),_:1})"#,
        false,
    );
}

#[test]
fn it_generates_mixed_slots_multiple() {
    // <test-component>
    //   <template v-slot:foo-bar>hello from slot</template>
    //   <template v-slot>hello from default<div>hello from div</div></template>
    //   <template v-slot:baz>hello from baz</template>
    // </test-component>
    test_out(
        ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: None,
            },
            vec![
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("foo-bar".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![Node::Text("hello from slot".into(), DUMMY_SP)],
                )),
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: None,
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![
                        Node::Text("hello from default".into(), DUMMY_SP),
                        Node::Element(ElementNode::new_with_children(
                            StartingTag {
                                tag_name: "div".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            vec![Node::Text("hello from div".into(), DUMMY_SP)],
                        )),
                    ],
                )),
                Node::Element(template_carrier(
                    StartingTag {
                        tag_name: "template".into(),
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("baz".into()),
                                value: None,
                            }),
                            ..Default::default()
                        })),
                    },
                    vec![Node::Text("hello from baz".into(), DUMMY_SP)],
                )),
            ],
            ElementKind::Component,
        ),
        r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),default:_withCtx(()=>[_createTextVNode("hello from default"),_createElementVNode("div",null,"hello from div")]),baz:_withCtx(()=>[_createTextVNode("hello from baz")]),_:1})"#,
        false,
    );
}

fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
    let out = crate::test_utils::generate_prepared_node(Node::Element(input), wrap_in_block);
    assert_eq!(crate::test_utils::to_str(out), expected)
}
