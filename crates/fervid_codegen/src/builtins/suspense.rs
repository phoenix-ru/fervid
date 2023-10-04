use fervid_core::{ElementNode, VueImports};
use swc_core::ecma::ast::{Expr, Ident};

use crate::CodegenContext;

impl CodegenContext {
    /// yeah, function name sounds funny
    pub fn generate_suspense(&mut self, element_node: &ElementNode) -> Expr {
        let span = element_node.span;

        // _Suspense
        let suspense_identifier = Expr::Ident(Ident {
            span,
            sym: self.get_and_add_import_ident(VueImports::Suspense),
            optional: false,
        });

        let suspense_attrs =
            self.generate_builtin_attrs(&element_node.starting_tag.attributes, span);

        let suspense_slots = self.generate_builtin_slots(element_node);

        self.generate_componentlike(
            suspense_identifier,
            suspense_attrs,
            suspense_slots,
            &element_node.patch_hints,
            true,
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{BuiltinType, ElementKind, StartingTag, Node, AttributeOrBinding};
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_empty_suspense() {
        // <suspense></suspense>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Suspense),
                starting_tag: StartingTag {
                    tag_name: "suspense".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Suspense))"#,
        )
    }

    #[test]
    fn it_generates_suspense_attrs() {
        // <suspense foo="bar" :baz="qux"></suspense>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Suspense),
                starting_tag: StartingTag {
                    tag_name: "suspense".into(),
                    attributes: vec![
                        AttributeOrBinding::RegularAttribute {
                            name: "foo".into(),
                            value: "bar".into(),
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
            r#"(_openBlock(),_createBlock(_Suspense,{foo:"bar",baz:qux}))"#,
        )
    }

    #[test]
    fn it_generates_suspense_children() {
        // <suspense>foobar</suspense>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Suspense),
                starting_tag: StartingTag {
                    tag_name: "suspense".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![Node::Text("foobar".into(), DUMMY_SP)],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Suspense,null,{"default":_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
        )
    }

    #[test]
    fn it_generates_full_suspense() {
        // <suspense foo="bar" :baz="qux">foobar</suspense>
        test_out(
            ElementNode {
                kind: ElementKind::Builtin(BuiltinType::Suspense),
                starting_tag: StartingTag {
                    tag_name: "suspense".into(),
                    attributes: vec![
                        AttributeOrBinding::RegularAttribute {
                            name: "foo".into(),
                            value: "bar".into(),
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
                children: vec![Node::Text("foobar".into(), DUMMY_SP)],
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"(_openBlock(),_createBlock(_Suspense,{foo:"bar",baz:qux},{"default":_withCtx(()=>[_createTextVNode("foobar")]),_:1}))"#,
        )
    }

    fn test_out(input: ElementNode, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_suspense(&input);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
