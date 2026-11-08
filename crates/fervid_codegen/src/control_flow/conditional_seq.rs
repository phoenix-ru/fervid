use fervid_core::{ConditionalNodeSequence, Node};
use swc_core::{
    common::{DUMMY_SP, Spanned},
    ecma::ast::{CondExpr, Expr},
};

use crate::context::CodegenContext;

impl CodegenContext {
    pub fn generate_conditional_seq(&mut self, conditional_seq: &ConditionalNodeSequence) -> Expr {
        let mut conditional_exprs = Vec::new();

        // First, push the `if` node
        let if_conditional = &conditional_seq.if_node;
        let if_expr = &if_conditional.condition;
        let if_node = &if_conditional.node;
        // let _has_js = transform_scoped(&mut if_expr, &self.scope_helper, if_node.template_scope);
        conditional_exprs.push(Box::new(if_expr.to_owned()));
        conditional_exprs.push(Box::new(
            self.generate_node(if_node, should_wrap_in_block(if_node)),
        ));

        // Then, push all the `else-if` nodes
        for else_if_conditional in conditional_seq.else_if_nodes.iter() {
            let else_if_expr = &else_if_conditional.condition;
            let else_if_node = &else_if_conditional.node;

            // let _has_js = transform_scoped(&mut else_if_expr, &self.scope_helper, else_if_node.template_scope);
            conditional_exprs.push(Box::new(else_if_expr.to_owned()));
            conditional_exprs.push(Box::new(
                self.generate_node(else_if_node, should_wrap_in_block(else_if_node)),
            ));
        }

        // Push either `else` or a comment node
        let else_expr = if let Some(ref else_node) = conditional_seq.else_node {
            self.generate_node(else_node, should_wrap_in_block(else_node))
        } else {
            let comment = if self.bindings_helper.is_prod {
                ""
            } else {
                "v-if"
            };
            self.generate_comment_vnode(comment, DUMMY_SP, true)
        };
        conditional_exprs.push(Box::new(else_expr));

        // And lastly, fold the results in triplets from the back
        // (..., condition, pos_branch, neg_branch) -> (..., expr)
        while conditional_exprs.len() >= 3 {
            let Some(negative_branch) = conditional_exprs.pop() else {
                unreachable!()
            };
            let Some(positive_branch) = conditional_exprs.pop() else {
                unreachable!()
            };
            let Some(condition) = conditional_exprs.pop() else {
                unreachable!()
            };

            // Combine 3 expressions into one ternary
            let ternary_expr = Expr::Cond(CondExpr {
                span: condition.span(),
                test: condition,
                cons: positive_branch,
                alt: negative_branch,
            });

            // Push back for the next iteration
            conditional_exprs.push(Box::new(ternary_expr));
        }

        // Get the final result and return it
        assert!(conditional_exprs.len() == 1);
        let Some(resulting_expr) = conditional_exprs.pop() else {
            unreachable!()
        };

        // I don't like the idea of dereferencing a Box, but the signature requires it
        *resulting_expr
    }
}

/// Omits wrapping conditional node in block when `v-for` directive is present (it will create its own block)
fn should_wrap_in_block(node: &Node) -> bool {
    match node {
        Node::Element(element_node) => element_node
            .starting_tag
            .directives
            .as_ref()
            .is_none_or(|d| d.v_for.is_none()),
        Node::For(_) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "new-pipeline")]
    use fervid_core::{
        AttributeOrBinding, BuiltinType, SfcDescriptor, SfcTemplateBlock, VBindDirective,
        VCustomDirective, VOnDirective, VueDirectives,
    };
    use fervid_core::{Conditional, ElementKind, ElementNode, Node, StartingTag};

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_v_if() {
        // <h1 v-if="foo || true">hello</h1>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo || true"),
                    node: Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello".into(), DUMMY_SP)],
                        template_scope: 0,
                        tag_type: ElementKind::Element,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                        codegen_node: None,
                    }),
                }),
                else_if_nodes: vec![],
                else_node: None,
                span: DUMMY_SP,
                key_base: 0,
            },
            r#"foo||true?(_openBlock(),_createElementBlock("h1",null,"hello")):_createCommentVNode("v-if",true)"#,
        )
    }

    #[test]
    fn it_generates_v_else() {
        // <h1 v-if="foo || true">hello</h1>
        // <h2 v-else>bye</h2>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo || true"),
                    node: Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello".into(), DUMMY_SP)],
                        template_scope: 0,
                        tag_type: ElementKind::Element,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                        codegen_node: None,
                    }),
                }),
                else_if_nodes: vec![],
                else_node: Some(Box::new(Node::Element(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "h2".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    children: vec![Node::Text("bye".into(), DUMMY_SP)],
                    template_scope: 0,
                    tag_type: ElementKind::Element,
                    patch_hints: Default::default(),
                    span: DUMMY_SP,
                    codegen_node: None,
                }))),
                span: DUMMY_SP,
                key_base: 0,
            },
            r#"foo||true?(_openBlock(),_createElementBlock("h1",null,"hello")):(_openBlock(),_createElementBlock("h2",null,"bye"))"#,
        )
    }

    #[test]
    fn it_generates_v_else_if() {
        // <h1 v-if="foo">hello</h1>
        // <h2 v-else-if="true">hi</h2>
        // <h3 v-else-if="undefined">bye</h2>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo"),
                    node: Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello".into(), DUMMY_SP)],
                        template_scope: 0,
                        tag_type: ElementKind::Element,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                        codegen_node: None,
                    }),
                }),
                else_if_nodes: vec![
                    Conditional {
                        condition: *js("true"),
                        node: Node::Element(ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h2".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            children: vec![Node::Text("hi".into(), DUMMY_SP)],
                            template_scope: 0,
                            tag_type: ElementKind::Element,
                            patch_hints: Default::default(),
                            span: DUMMY_SP,
                            codegen_node: None,
                        }),
                    },
                    Conditional {
                        condition: *js("undefined"),
                        node: Node::Element(ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h3".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            children: vec![Node::Text("bye".into(), DUMMY_SP)],
                            template_scope: 0,
                            tag_type: ElementKind::Element,
                            patch_hints: Default::default(),
                            span: DUMMY_SP,
                            codegen_node: None,
                        }),
                    },
                ],
                else_node: None,
                span: DUMMY_SP,
                key_base: 0,
            },
            r#"foo?(_openBlock(),_createElementBlock("h1",null,"hello")):true?(_openBlock(),_createElementBlock("h2",null,"hi")):undefined?(_openBlock(),_createElementBlock("h3",null,"bye")):_createCommentVNode("v-if",true)"#,
        )
    }

    #[test]
    fn it_generates_complex() {
        // <h1 v-if="foo">hello</h1>
        // <h2 v-else-if="true">hi</h2>
        // <h3 v-else-if="undefined">good morning</h2>
        // <h4 v-else>bye</h4>
        test_out(
            ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *js("foo"),
                    node: Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "h1".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello".into(), DUMMY_SP)],
                        template_scope: 0,
                        tag_type: ElementKind::Element,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                        codegen_node: None,
                    }),
                }),
                else_if_nodes: vec![
                    Conditional {
                        condition: *js("true"),
                        node: Node::Element(ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h2".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            children: vec![Node::Text("hi".into(), DUMMY_SP)],
                            template_scope: 0,
                            tag_type: ElementKind::Element,
                            patch_hints: Default::default(),
                            span: DUMMY_SP,
                            codegen_node: None,
                        }),
                    },
                    Conditional {
                        condition: *js("undefined"),
                        node: Node::Element(ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "h3".into(),
                                attributes: vec![],
                                directives: None,
                            },
                            children: vec![Node::Text("good morning".into(), DUMMY_SP)],
                            template_scope: 0,
                            tag_type: ElementKind::Element,
                            patch_hints: Default::default(),
                            span: DUMMY_SP,
                            codegen_node: None,
                        }),
                    },
                ],
                else_node: Some(Box::new(Node::Element(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "h4".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    children: vec![Node::Text("bye".into(), DUMMY_SP)],
                    template_scope: 0,
                    tag_type: ElementKind::Element,
                    patch_hints: Default::default(),
                    span: DUMMY_SP,
                    codegen_node: None,
                }))),
                span: DUMMY_SP,
                key_base: 0,
            },
            r#"foo?(_openBlock(),_createElementBlock("h1",null,"hello")):true?(_openBlock(),_createElementBlock("h2",null,"hi")):undefined?(_openBlock(),_createElementBlock("h3",null,"good morning")):(_openBlock(),_createElementBlock("h4",null,"bye"))"#,
        )
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L98-L114
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn component_v_if() {
        // <Component v-if="ok"></Component>
        let mut component = conditional_element(
            "Component",
            VueDirectives {
                v_if: Some(js("ok")),
                ..Default::default()
            },
        );
        let Node::Element(element) = &mut component else {
            panic!("component branch should contain an element");
        };
        element.tag_type = ElementKind::Component;

        assert_eq!(
            transform_and_generate_roots(vec![component]),
            "_ctx.ok?(_openBlock(),_createBlock(_component_Component,{key:0})):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L180-L212
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn comment_between_branches() {
        // <div v-if="ok"/>
        // <!--foo-->
        // <p v-else-if="orNot"/>
        // <!--bar-->
        // <template v-else>fine</template>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                Node::Comment("foo".into(), DUMMY_SP),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else_if: Some(js("orNot")),
                        ..Default::default()
                    },
                ),
                Node::Comment("bar".into(), DUMMY_SP),
                conditional_template_else(vec![Node::Text("fine".into(), DUMMY_SP)]),
            ]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):_ctx.orNot?(_openBlock(),_createElementBlock(_Fragment,{key:1},[_createCommentVNode(\"foo\"),_createElementVNode(\"p\")],2112)):(_openBlock(),_createElementBlock(_Fragment,{key:2},[_createCommentVNode(\"bar\"),_createTextVNode(\"fine\")],64))"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L466-L481
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn basic_v_if() {
        // <div v-if="ok"/>
        assert_eq!(
            transform_and_generate_roots(vec![conditional_element(
                "div",
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
            )]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L483-L503
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn template_v_if() {
        // <template v-if="ok"><div/>hello<p/></template>
        let element = |tag_name| {
            Node::Element(ElementNode::new(StartingTag {
                tag_name,
                attributes: vec![],
                directives: None,
            }))
        };
        assert_eq!(
            transform_and_generate_template_if(vec![
                element("div".into()),
                Node::Text("hello".into(), DUMMY_SP),
                element("p".into()),
            ]),
            "_ctx.ok?(_openBlock(),_createElementBlock(_Fragment,{key:0},[_createElementVNode(\"div\"),_createTextVNode(\"hello\"),_createElementVNode(\"p\")],64)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Additional single-text variant of Vue's template v-if test above
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn template_v_if_with_single_text() {
        // <template v-if="ok">hello</template>
        assert_eq!(
            transform_and_generate_template_if(vec![Node::Text("hello".into(), DUMMY_SP)]),
            "_ctx.ok?(_openBlock(),_createElementBlock(_Fragment,{key:0},[_createTextVNode(\"hello\")],64)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L505-L523
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn template_v_if_with_single_slot_child() {
        // <template v-if="ok"><slot/></template>
        assert_eq!(
            transform_and_generate_template_if(vec![slot_outlet(vec![], false)]),
            "_ctx.ok?_renderSlot(_ctx.$slots,\"default\",{},undefined,undefined,0):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L525-L543
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_if_on_slot() {
        // <slot v-if="ok"></slot>
        assert_eq!(
            transform_and_generate_roots(vec![slot_outlet(vec![], true)]),
            "_ctx.ok?_renderSlot(_ctx.$slots,\"default\",{},undefined,undefined,0):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L545-L570
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_if_on_slot_with_v_bind() {
        // <slot v-if="ok" v-bind="items"></slot>
        let binding = AttributeOrBinding::VBind(VBindDirective {
            argument: None,
            value: js("items"),
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: DUMMY_SP,
        });
        assert_eq!(
            transform_and_generate_roots(vec![slot_outlet(vec![binding], true)]),
            "_ctx.ok?_renderSlot(_ctx.$slots,\"default\",_normalizeProps(_guardReactiveProps(_ctx.items)),undefined,undefined,0):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L572-L587
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_if_else() {
        // <div v-if="ok"/><p v-else/>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):(_openBlock(),_createElementBlock(\"p\",{key:1}))"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L589-L605
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_if_else_if() {
        // <div v-if="ok"/><p v-else-if="orNot"/>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else_if: Some(js("orNot")),
                        ..Default::default()
                    },
                ),
            ]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):_ctx.orNot?(_openBlock(),_createElementBlock(\"p\",{key:1})):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L607-L635
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_if_else_if_else() {
        // <div v-if="ok"/><p v-else-if="orNot"/><template v-else>fine</template>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else_if: Some(js("orNot")),
                        ..Default::default()
                    },
                ),
                conditional_template_else(vec![Node::Text("fine".into(), DUMMY_SP)]),
            ]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):_ctx.orNot?(_openBlock(),_createElementBlock(\"p\",{key:1})):(_openBlock(),_createElementBlock(_Fragment,{key:2},[_createTextVNode(\"fine\")],64))"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L637-L660
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn multiple_v_if_that_are_sibling_nodes_should_have_different_keys() {
        // <div v-if="ok"/><p v-if="orNot"/>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_if: Some(js("orNot")),
                        ..Default::default()
                    },
                ),
            ]),
            "(_openBlock(),_createElementBlock(_Fragment,null,[_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):_createCommentVNode(\"v-if\",true),_ctx.orNot?(_openBlock(),_createElementBlock(\"p\",{key:1})):_createCommentVNode(\"v-if\",true)],64))"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L662-L697
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn increasing_key_v_if_else_if_else() {
        // <div v-if="ok"/><p v-else/><div v-if="another"/><p v-else-if="orNot"/><p v-else/>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("another")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else_if: Some(js("orNot")),
                        ..Default::default()
                    },
                ),
                conditional_element(
                    "p",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ]),
            "(_openBlock(),_createElementBlock(_Fragment,null,[_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):(_openBlock(),_createElementBlock(\"p\",{key:1})),_ctx.another?(_openBlock(),_createElementBlock(\"div\",{key:2})):_ctx.orNot?(_openBlock(),_createElementBlock(\"p\",{key:3})):(_openBlock(),_createElementBlock(\"p\",{key:4}))],64))"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L699-L718
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn key_injection_only_v_bind() {
        // <div v-if="ok" v-bind="obj"/>
        assert_eq!(
            transform_and_generate_roots(vec![conditional_element_with_attributes(
                "div",
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![spread_obj()],
            )]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",_normalizeProps(_mergeProps({key:0},_ctx.obj)),null,16)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L720-L736
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn key_injection_before_v_bind() {
        // <div v-if="ok" id="foo" v-bind="obj"/>
        assert_eq!(
            transform_and_generate_roots(vec![conditional_element_with_attributes(
                "div",
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![regular_attribute("id", "foo"), spread_obj()],
            )]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",_mergeProps({key:0,id:\"foo\"},_ctx.obj),null,16)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L738-L754
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn key_injection_after_v_bind() {
        // <div v-if="ok" v-bind="obj" id="foo"/>
        assert_eq!(
            transform_and_generate_roots(vec![conditional_element_with_attributes(
                "div",
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![spread_obj(), regular_attribute("id", "foo")],
            )]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",_mergeProps({key:0},_ctx.obj,{id:\"foo\"}),null,16)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L756-L763
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn key_injection_with_custom_directive() {
        // <div v-if="ok" v-foo />
        assert_eq!(
            transform_and_generate_roots(vec![conditional_element(
                "div",
                VueDirectives {
                    v_if: Some(js("ok")),
                    custom: vec![VCustomDirective {
                        name: "foo".into(),
                        value: None,
                        argument: None,
                        modifiers: vec![],
                        span: DUMMY_SP,
                    }],
                    ..Default::default()
                },
            )]),
            "_ctx.ok?_withDirectives((_openBlock(),_createElementBlock(\"div\",{key:0},null,512)),[[_directive_foo]]):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L765-L781
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn avoid_duplicate_keys() {
        // <div v-if="ok" key="custom_key" v-bind="obj"/>
        assert_eq!(
            transform_and_generate_roots(vec![conditional_element_with_attributes(
                "div",
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![regular_attribute("key", "custom_key"), spread_obj()],
            )]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",_mergeProps({key:\"custom_key\"},_ctx.obj),null,16)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L783-L802
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn with_spaces_between_branches() {
        // <div v-if="ok"/> <div v-else-if="no"/> <div v-else/>
        assert_eq!(
            transform_and_generate_roots(vec![
                conditional_element(
                    "div",
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
                Node::Text(" ".into(), DUMMY_SP),
                conditional_element(
                    "div",
                    VueDirectives {
                        v_else_if: Some(js("no")),
                        ..Default::default()
                    },
                ),
                Node::Text(" ".into(), DUMMY_SP),
                conditional_element(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ]),
            "_ctx.ok?(_openBlock(),_createElementBlock(\"div\",{key:0})):_ctx.no?(_openBlock(),_createElementBlock(\"div\",{key:1})):(_openBlock(),_createElementBlock(\"div\",{key:2}))"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L804-L848
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn with_comments() {
        // <template v-if="ok">
        //   <!--comment1-->
        //   <div v-if="ok2">
        //     <!--comment2-->
        //   </div>
        //   <!--comment3-->
        //   <b v-else/>
        //   <!--comment4-->
        //   <p/>
        // </template>
        assert_eq!(
            transform_and_generate_roots(vec![template_if_with_comments()]),
            "_ctx.ok?(_openBlock(),_createElementBlock(_Fragment,{key:0},[_createCommentVNode(\"comment1\"),_ctx.ok2?(_openBlock(),_createElementBlock(\"div\",{key:0},[_createCommentVNode(\"comment2\")])):(_openBlock(),_createElementBlock(_Fragment,{key:1},[_createCommentVNode(\"comment3\"),_createElementVNode(\"b\")],2112)),_createCommentVNode(\"comment4\"),_createElementVNode(\"p\")],64)):_createCommentVNode(\"v-if\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L850-L869
    // Vue drops inter-branch comments in production, even when comments: true
    // https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/src/transforms/vIf.ts#L130-L139
    // Vue also emits an empty fallback comment in production
    // https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/src/transforms/vIf.ts#L224-L234
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn should_parse_correctly_with_comments_true_in_prod() {
        // <template v-if="ok">
        //   <!--comment1-->
        //   <div v-if="ok2">
        //     <!--comment2-->
        //   </div>
        //   <!--comment3-->
        //   <b v-else/>
        //   <!--comment4-->
        //   <p/>
        // </template>
        assert_eq!(
            transform_and_generate_roots_in_mode(vec![template_if_with_comments()], true),
            "_ctx.ok?(_openBlock(),_createElementBlock(_Fragment,{key:0},[_createCommentVNode(\"comment1\"),_ctx.ok2?(_openBlock(),_createElementBlock(\"div\",{key:0},[_createCommentVNode(\"comment2\")])):(_openBlock(),_createElementBlock(\"b\",{key:1})),_createCommentVNode(\"comment4\"),_createElementVNode(\"p\")],64)):_createCommentVNode(\"\",true)"
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L872-L890
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn v_on_with_v_if() {
        // <button v-on="{ click: clickEvent }" v-if="true">w/ v-if</button>
        let mut node = conditional_element_with_attributes(
            "button",
            VueDirectives {
                v_if: Some(js("true")),
                ..Default::default()
            },
            vec![AttributeOrBinding::VOn(VOnDirective {
                event: None,
                handler: Some(js("{ click: clickEvent }")),
                modifiers: vec![],
                span: DUMMY_SP,
            })],
        );
        let Node::Element(element) = &mut node else {
            panic!("button branch should contain an element");
        };
        element
            .children
            .push(Node::Text("w/ v-if".into(), DUMMY_SP));
        assert_eq!(
            transform_and_generate_roots(vec![node]),
            "true?(_openBlock(),_createElementBlock(\"button\",_mergeProps({key:0},_toHandlers({click:_ctx.clickEvent},true)),\"w/ v-if\",16)):_createCommentVNode(\"v-if\",true)"
        );
    }

    #[cfg(feature = "new-pipeline")]
    fn transform_and_generate_template_if(children: Vec<Node>) -> String {
        transform_and_generate_roots(vec![Node::Element(
            ElementNode::new_with_children_and_type(
                StartingTag {
                    tag_name: "template".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    })),
                },
                children,
                ElementKind::Template,
            ),
        )])
    }

    #[cfg(feature = "new-pipeline")]
    fn conditional_template_else(children: Vec<Node>) -> Node {
        Node::Element(ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                })),
            },
            children,
            ElementKind::Template,
        ))
    }

    #[cfg(feature = "new-pipeline")]
    fn template_if_with_comments() -> Node {
        let mut nested_if = conditional_element(
            "div",
            VueDirectives {
                v_if: Some(js("ok2")),
                ..Default::default()
            },
        );
        let Node::Element(element) = &mut nested_if else {
            panic!("nested conditional should contain an element");
        };
        element
            .children
            .push(Node::Comment("comment2".into(), DUMMY_SP));
        Node::Element(ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                })),
            },
            vec![
                Node::Comment("comment1".into(), DUMMY_SP),
                nested_if,
                Node::Comment("comment3".into(), DUMMY_SP),
                conditional_element(
                    "b",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
                Node::Comment("comment4".into(), DUMMY_SP),
                Node::Element(ElementNode::new(StartingTag {
                    tag_name: "p".into(),
                    attributes: vec![],
                    directives: None,
                })),
            ],
            ElementKind::Template,
        ))
    }

    #[cfg(feature = "new-pipeline")]
    fn conditional_element(tag_name: &str, directives: VueDirectives) -> Node {
        conditional_element_with_attributes(tag_name, directives, vec![])
    }

    #[cfg(feature = "new-pipeline")]
    fn conditional_element_with_attributes(
        tag_name: &str,
        directives: VueDirectives,
        attributes: Vec<AttributeOrBinding>,
    ) -> Node {
        Node::Element(ElementNode::new(StartingTag {
            tag_name: tag_name.into(),
            attributes,
            directives: Some(Box::new(directives)),
        }))
    }

    #[cfg(feature = "new-pipeline")]
    fn slot_outlet(attributes: Vec<AttributeOrBinding>, v_if: bool) -> Node {
        let mut node = conditional_element_with_attributes(
            "slot",
            VueDirectives {
                v_if: v_if.then(|| js("ok")),
                ..Default::default()
            },
            attributes,
        );
        let Node::Element(element) = &mut node else {
            panic!("slot outlet should contain an element");
        };
        element.tag_type = ElementKind::Builtin(BuiltinType::Slot);
        node
    }

    #[cfg(feature = "new-pipeline")]
    fn regular_attribute(name: &str, value: &str) -> AttributeOrBinding {
        AttributeOrBinding::RegularAttribute {
            name: name.into(),
            value: Some(value.into()),
            span: DUMMY_SP,
        }
    }

    #[cfg(feature = "new-pipeline")]
    fn spread_obj() -> AttributeOrBinding {
        AttributeOrBinding::VBind(VBindDirective {
            argument: None,
            value: js("obj"),
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: DUMMY_SP,
        })
    }

    #[cfg(feature = "new-pipeline")]
    fn transform_and_generate_roots(roots: Vec<Node>) -> String {
        transform_and_generate_roots_in_mode(roots, false)
    }

    #[cfg(feature = "new-pipeline")]
    fn transform_and_generate_roots_in_mode(roots: Vec<Node>, is_prod: bool) -> String {
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots,
            span: DUMMY_SP,
        };
        let descriptor = SfcDescriptor::default();
        let options = fervid_transform::TransformSfcOptions {
            is_prod,
            is_ce: false,
            props_destructure: Default::default(),
            scope_id: "",
            filename: "anonymous.vue",
            transform_asset_urls: Default::default(),
            directive_transforms: Default::default(),
            node_transforms: Default::default(),
        };
        let mut transform_ctx = fervid_transform::TransformSfcContext::new(&descriptor, &options);
        fervid_transform::template::transform_and_record_template(
            &mut template,
            &mut transform_ctx,
        );
        assert!(
            transform_ctx.errors.is_empty(),
            "{:#?}",
            transform_ctx.errors
        );

        let mut codegen_ctx = CodegenContext::with_bindings_helper(transform_ctx.bindings_helper);
        crate::test_utils::to_str(
            codegen_ctx
                .generate_sfc_template(&template)
                .expect("conditional template root should generate code"),
        )
    }

    fn test_out(input: ConditionalNodeSequence, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_conditional_seq(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
