use fervid_core::fervid_atom;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, Ident, KeyValueProp, Prop, PropName, PropOrSpread},
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the `v-html` directive
    ///
    /// # Example
    /// `v-html="foo + bar"` will generate `innerHTML: foo + bar` (without transforms).
    ///
    /// With transforms the node must handle
    /// patch flags: `8 /* PROPS */` and `"innerHTML"` in props array.
    pub fn generate_v_html(&self, expr: &Expr) -> PropOrSpread {
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(Ident {
                span: DUMMY_SP, // TODO?
                sym: fervid_atom!("innerHTML"),
                optional: false,
            }),
            value: Box::new(expr.to_owned()),
        })))
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{ElementKind, ElementNode, StartingTag, VueDirectives};
    use swc_core::ecma::ast::BinExpr;

    use super::*;

    #[test]
    fn it_generates_v_html_on_component() {
        test_out(
            // <test-component v-html="foo + bar" />
            ElementNode {
                children: vec![],
                kind: ElementKind::Component,
                starting_tag: StartingTag {
                    tag_name: "test-component".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_html: Some(Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: swc_core::ecma::ast::BinaryOp::Add,
                            left: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: fervid_atom!("foo"),
                                optional: false,
                            })),
                            right: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: fervid_atom!("bar"),
                                optional: false,
                            })),
                        }))),
                        ..Default::default()
                    })),
                },
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createVNode(_component_test_component,{innerHTML:foo+bar})"#,
            false,
        )
    }

    #[test]
    fn it_generates_v_html_on_element() {
        test_out(
            // <h1 v-html="foo + bar" />
            ElementNode {
                children: vec![],
                kind: ElementKind::Element,
                starting_tag: StartingTag {
                    tag_name: "h1".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_html: Some(Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: swc_core::ecma::ast::BinaryOp::Add,
                            left: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: fervid_atom!("foo"),
                                optional: false,
                            })),
                            right: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: fervid_atom!("bar"),
                                optional: false,
                            })),
                        }))),
                        ..Default::default()
                    })),
                },
                template_scope: 0,
                patch_hints: Default::default(),
                span: DUMMY_SP,
            },
            r#"_createElementVNode("h1",{innerHTML:foo+bar})"#,
            false,
        )
    }

    fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
        let is_component = matches!(input.kind, ElementKind::Component);

        let mut ctx = CodegenContext::default();
        let out = if is_component {
            ctx.generate_component_vnode(&input, wrap_in_block)
        } else {
            ctx.generate_element_vnode(&input, wrap_in_block)
        };
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
