use fervid_core::fervid_atom;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, IdentName, KeyValueProp, Prop, PropName, PropOrSpread},
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the `v-text` directive
    ///
    /// # Example
    /// `v-text="foo + bar"` will generate `textContent: foo + bar` (without transforms)
    pub fn generate_v_text(&self, expr: &Expr) -> PropOrSpread {
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(IdentName {
                span: DUMMY_SP, // TODO
                sym: fervid_atom!("textContent"),
            }),
            value: Box::new(expr.to_owned()),
        })))
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        ElementKind, ElementNode, IntoIdent, StartingTag, VueDirectives, fervid_atom,
    };
    use swc_core::{
        common::DUMMY_SP,
        ecma::ast::{BinExpr, Expr},
    };

    use crate::test_utils::js;

    #[test]
    fn it_generates_v_text_on_component() {
        test_out(
            // <test-component v-text="foo + bar" />
            ElementNode {
                children: vec![],
                tag_type: ElementKind::Component,
                starting_tag: StartingTag {
                    tag_name: "test-component".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_text: Some(Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: swc_core::ecma::ast::BinaryOp::Add,
                            left: Box::new(Expr::Ident(fervid_atom!("foo").into_ident())),
                            right: Box::new(Expr::Ident(fervid_atom!("bar").into_ident())),
                        }))),
                        ..Default::default()
                    })),
                },
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            },
            r#"_createVNode(_component_test_component,{textContent:_toDisplayString(foo+bar)},null,8,["textContent"])"#,
            false,
        )
    }

    #[test]
    fn it_generates_v_text_on_element() {
        test_out(
            // <h1 v-text="foo + bar" />
            ElementNode {
                children: vec![],
                tag_type: ElementKind::Element,
                starting_tag: StartingTag {
                    tag_name: "h1".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_text: Some(js("foo + bar")),
                        ..Default::default()
                    })),
                },
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            },
            r#"_createElementVNode("h1",{textContent:_toDisplayString(foo+bar)},null,8,["textContent"])"#,
            false,
        )
    }

    fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
        let out = crate::test_utils::generate_prepared_node(
            fervid_core::Node::Element(input),
            wrap_in_block,
        );
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
