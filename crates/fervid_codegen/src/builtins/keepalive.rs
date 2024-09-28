use fervid_core::{ElementNode, IntoIdent, PatchFlags, PatchFlagsSet, PatchHints, VueImports};
use swc_core::ecma::ast::{ArrayLit, Expr, ExprOrSpread};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates `(_openBlock(), _createBlock(_KeepAlive, null, [keepalive_children], 1024))`
    pub fn generate_keepalive(&mut self, element_node: &ElementNode) -> Expr {
        let span = element_node.span;

        // _KeepAlive
        let keepalive_identifier = Expr::Ident(
            self.get_and_add_import_ident(VueImports::KeepAlive)
                .into_ident_spanned(span),
        );

        let keepalive_attrs =
            self.generate_builtin_attrs(&element_node.starting_tag.attributes, span);

        let generated_children = self.generate_element_children(element_node, false);
        let keepalive_children = if generated_children.0.len() != 0 {
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

        let should_use_block = keepalive_children.is_some();

        let patch_hints = PatchHints {
            flags: if should_use_block {
                PatchFlagsSet::from(PatchFlags::DynamicSlots)
            } else {
                PatchFlagsSet::default()
            },
            props: vec![],
            should_use_block,
        };

        self.generate_componentlike(
            keepalive_identifier,
            keepalive_attrs,
            keepalive_children,
            &patch_hints,
            should_use_block,
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{BuiltinType, ElementKind, Node, StartingTag};
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{regular_attribute, v_bind_attribute};

    use super::*;

    #[test]
    fn it_generates_empty_keepalive() {
        // <keep-alive></keep-alive>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::KeepAlive),
                starting_tag: StartingTag {
                    tag_name: "keep-alive".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createVNode(_KeepAlive)"#,
        )
    }

    #[test]
    fn it_generates_keepalive_attrs() {
        // <keep-alive foo="bar" :baz="qux"></keep-alive>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::KeepAlive),
                starting_tag: StartingTag {
                    tag_name: "keep-alive".into(),
                    attributes: vec![
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
            r#"_createVNode(_KeepAlive,{foo:"bar",baz:qux})"#,
        )
    }

    #[test]
    fn it_generates_keepalive_children() {
        // <keep-alive>foobar</keep-alive>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::KeepAlive),
                starting_tag: StartingTag {
                    tag_name: "keep-alive".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![Node::Text("foobar".into(), DUMMY_SP)],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_KeepAlive,null,[_createTextVNode("foobar")],1024))"#,
        )
    }

    #[test]
    fn it_generates_full_keepalive() {
        // <keep-alive foo="bar" :baz="qux">foobar</keep-alive>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::KeepAlive),
                starting_tag: StartingTag {
                    tag_name: "keep-alive".into(),
                    attributes: vec![
                        regular_attribute("foo", "bar"),
                        v_bind_attribute("baz", "qux"),
                    ],
                    directives: None,
                },
                children: vec![Node::Text("foobar".into(), DUMMY_SP)],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_KeepAlive,{foo:"bar",baz:qux},[_createTextVNode("foobar")],1024))"#,
        )
    }

    fn test_out(input: ElementNode, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_keepalive(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
