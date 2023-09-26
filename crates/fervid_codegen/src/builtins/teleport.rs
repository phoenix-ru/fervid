use fervid_core::{ElementNode, VueImports};
use swc_core::ecma::ast::{ArrayLit, Expr, ExprOrSpread, Ident};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates `(_openBlock(), _createBlock(_Teleport, null, [teleport_children]))`
    pub fn generate_teleport(&mut self, element_node: &ElementNode) -> Expr {
        let span = element_node.span;

        // _Teleport
        let teleport_identifier = Expr::Ident(Ident {
            span,
            sym: self.get_and_add_import_ident(VueImports::Teleport),
            optional: false,
        });

        let teleport_attrs =
            self.generate_builtin_attrs(&element_node.starting_tag.attributes, span);

        let generated_children = self.generate_element_children(element_node, false);
        let teleport_children = if generated_children.0.len() != 0 {
            Some(Expr::Array(ArrayLit {
                span,
                elems: generated_children
                    .0
                    .into_iter()
                    .map(|c| {
                        Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(c),
                        })
                    })
                    .collect(),
            }))
        } else {
            None
        };

        self.generate_componentlike(
            teleport_identifier,
            teleport_attrs,
            teleport_children,
            &element_node.patch_hints,
            true,
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{AttributeOrBinding, BuiltinType, ElementKind, Node, StartingTag};
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_empty_teleport() {
        // <teleport></teleport>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Teleport),
                starting_tag: StartingTag {
                    tag_name: "teleport",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Teleport))"#,
        )
    }

    #[test]
    fn it_generates_teleport_attrs() {
        // <teleport foo="bar" :baz="qux"></teleport>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Teleport),
                starting_tag: StartingTag {
                    tag_name: "teleport",
                    attributes: vec![
                        AttributeOrBinding::RegularAttribute {
                            name: "foo",
                            value: "bar",
                        },
                        AttributeOrBinding::VBind(fervid_core::VBindDirective {
                            argument: Some("baz".into()),
                            value: js("qux"),
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        }),
                    ],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Teleport,{foo:"bar",baz:qux}))"#,
        )
    }

    #[test]
    fn it_generates_teleport_children() {
        // <teleport>foobar</teleport>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Teleport),
                starting_tag: StartingTag {
                    tag_name: "teleport",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![Node::Text("foobar")],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Teleport,null,[_createTextVNode("foobar")]))"#,
        )
    }

    #[test]
    fn it_generates_full_teleport() {
        // <teleport foo="bar" :baz="qux">foobar</teleport>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Teleport),
                starting_tag: StartingTag {
                    tag_name: "teleport",
                    attributes: vec![
                        AttributeOrBinding::RegularAttribute {
                            name: "foo",
                            value: "bar",
                        },
                        AttributeOrBinding::VBind(fervid_core::VBindDirective {
                            argument: Some("baz".into()),
                            value: js("qux"),
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        }),
                    ],
                    directives: None,
                },
                children: vec![Node::Text("foobar")],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Teleport,{foo:"bar",baz:qux},[_createTextVNode("foobar")]))"#,
        )
    }

    fn test_out(input: ElementNode, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_teleport(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
