//! This module covers the `<component>` Vue builtin.
//! Please do not confuse with the user components.

use fervid_core::{
    check_attribute_name, AttributeOrBinding, ElementNode, StrOrExpr, VBindDirective, VueImports,
};
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, ObjectLit, PropOrSpread, Str,
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the `<component>` builtin
    pub fn generate_component_builtin(&mut self, element_node: &ElementNode) -> Expr {
        let span = element_node.span;

        // Shortcut
        let attributes = &element_node.starting_tag.attributes;

        // Find the `is` or `:is` attribute of the `<component>`
        let component_is_attribute_idx = attributes
            .iter()
            .position(|attr| check_attribute_name(attr, "is"))
            .expect("<component> should always have `is` attribute");

        let component_is_attribute = &attributes[component_is_attribute_idx];

        // Expression to put as the first argument to `resolveDynamicComponent()`
        let is_attribute_expr = match component_is_attribute {
            AttributeOrBinding::RegularAttribute { name, value, span } if name == "is" => {
                Expr::Lit(Lit::Str(Str {
                    span: *span,
                    value: value.to_owned(),
                    raw: None,
                }))
            }

            AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(name)),
                value,
                ..
            }) if name == "is" => (**value).to_owned(),

            _ => unreachable!(),
        };

        // resolveDynamicComponent(is_attribute)
        let identifier = Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::ResolveDynamicComponent),
                optional: false,
            }))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(is_attribute_expr),
            }],
            type_args: None,
        });

        let component_builtin_attrs: Option<Expr> = if attributes.len() != 1 {
            // Split attributes at before `is` and after `is`.
            // This way, we exclude `is` and avoid any prior sorting
            let attrs_first_half = &attributes[..component_is_attribute_idx];
            let attrs_second_half = &attributes[(component_is_attribute_idx + 1)..];

            let mut attrs: Vec<PropOrSpread> = Vec::with_capacity(attributes.len() - 1);

            // TODO Use hints for a patch flag?
            self.generate_attributes(attrs_first_half, &mut attrs);
            self.generate_attributes(attrs_second_half, &mut attrs);

            Some(Expr::Object(ObjectLit { span, props: attrs }))
        } else {
            None
        };

        // TODO
        // 2. Do not identify the node as a builtin if it does not have `is` attribute;
        // 7. Update the README and the progress.

        let component_builtin_slots = self.generate_builtin_slots(element_node);

        self.generate_componentlike(
            identifier,
            component_builtin_attrs,
            component_builtin_slots,
            &element_node.patch_hints,
            true,
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use fervid_core::{BuiltinType, ElementKind, Node, StartingTag, VSlotDirective, VueDirectives};
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{regular_attribute, v_bind_attribute};

    use super::*;

    #[test]
    fn it_panics_at_empty_component() {
        test_panic(
            || {
                // <component></component>
                test_out(
                    ElementNode {
                        kind: ElementKind::Builtin(BuiltinType::Component),
                        starting_tag: StartingTag {
                            tag_name: "component".into(),
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![],
                        template_scope: 0,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    },
                    r#""#,
                );
            },
            "<component> should always have `is` attribute",
        );
    }

    #[test]
    fn it_generates_component_is_static() {
        // <component is="div"></component>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Component),
                starting_tag: StartingTag {
                    tag_name: "component".into(),
                    attributes: vec![regular_attribute("is", "div")],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div")))"#,
        );
    }

    #[test]
    fn it_generates_component_is_binding() {
        // <component :is="foo"></component>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Component),
                starting_tag: StartingTag {
                    tag_name: "component".into(),
                    attributes: vec![v_bind_attribute("is", "foo")],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_resolveDynamicComponent(foo)))"#,
        );
    }

    #[test]
    fn it_generates_component_builtin_attrs() {
        // <component is="div" foo="bar" :baz="qux"></component>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Component),
                starting_tag: StartingTag {
                    tag_name: "component".into(),
                    attributes: vec![
                        regular_attribute("is", "div"),
                        regular_attribute("foo", "bar"),
                        v_bind_attribute("baz", "qux"),
                    ],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),{foo:"bar",baz:qux}))"#,
        )
    }

    #[test]
    fn it_generates_component_builtin_default_slot() {
        // <component is="div">foobar</component>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Component),
                starting_tag: StartingTag {
                    tag_name: "component".into(),
                    attributes: vec![regular_attribute("is", "div")],
                    directives: None,
                },
                children: vec![Node::Text("foobar".into(), DUMMY_SP)],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),null,{"default":_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
        )
    }

    #[test]
    fn it_generates_component_builtin_named_slot() {
        // <component is="div">
        //   <template v-slot:named>foobar</template>
        // </component>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Component),
                starting_tag: StartingTag {
                    tag_name: "component".into(),
                    attributes: vec![regular_attribute("is", "div")],
                    directives: None,
                },
                children: vec![Node::Element(ElementNode {
                    kind: ElementKind::Element,
                    starting_tag: StartingTag {
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
                    children: vec![Node::Text("foobar".into(), DUMMY_SP)],
                    template_scope: 0,
                    patch_hints: Default::default(),
                    span: DUMMY_SP,
                })],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
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
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Component),
                starting_tag: StartingTag {
                    tag_name: "component".into(),
                    attributes: vec![
                        regular_attribute("is", "div"),
                        regular_attribute("foo", "bar"),
                        v_bind_attribute("baz", "qux"),
                    ],
                    directives: None,
                },
                children: vec![
                    Node::Text("foobar".into(), DUMMY_SP),
                    Node::Element(ElementNode {
                        kind: ElementKind::Element,
                        starting_tag: StartingTag {
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
                        children: vec![Node::Text("bazqux".into(), DUMMY_SP)],
                        template_scope: 0,
                        patch_hints: Default::default(),
                        span: DUMMY_SP,
                    }),
                ],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_resolveDynamicComponent("div"),{foo:"bar",baz:qux},{named:_withCtx(()=>[_createTextVNode("bazqux")]),"default":_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
        )
    }

    fn test_out(input: ElementNode, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_component_builtin(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }

    // Adapted with https://stackoverflow.com/a/59211519 to silence an error message
    fn test_panic<F: FnOnce() -> R + std::panic::UnwindSafe, R: Debug>(f: F, expected_err: &str) {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let err = std::panic::catch_unwind(f).unwrap_err();
        std::panic::set_hook(prev_hook);

        let panic_msg = panic_message::get_panic_message(&err);
        assert!(panic_msg.is_some());
        assert_eq!(expected_err, panic_msg.unwrap());
    }
}
