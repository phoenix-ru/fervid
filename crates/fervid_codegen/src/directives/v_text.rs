use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{Expr, Ident, KeyValueProp, Prop, PropName, PropOrSpread},
        atoms::JsWord,
    },
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates the `v-text` directive
    ///
    /// # Example
    /// `v-text="foo + bar"` will generate `textContent: foo + bar` (without transforms)
    pub fn generate_v_text(&self, expr: &Expr) -> PropOrSpread {
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(Ident {
                span: DUMMY_SP, // TODO
                sym: JsWord::from("textContent"),
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
    fn it_generates_v_text_on_component() {
        test_out(
            // <test-component v-text="foo + bar" />
            ElementNode {
                children: vec![],
                kind: ElementKind::Component,
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_text: Some(Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: swc_core::ecma::ast::BinaryOp::Add,
                            left: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: JsWord::from("foo"),
                                optional: false,
                            })),
                            right: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: JsWord::from("bar"),
                                optional: false,
                            })),
                        }))),
                        ..Default::default()
                    })),
                },
                template_scope: 0,
            },
            r#"_createVNode(_component_test_component,{textContent:foo+bar})"#,
            false,
        )
    }

    #[test]
    fn it_generates_v_text_on_element() {
        test_out(
            // <h1 v-text="foo + bar" />
            ElementNode {
                children: vec![],
                kind: ElementKind::Element,
                starting_tag: StartingTag {
                    tag_name: "h1",
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_text: Some(Box::new(Expr::Bin(BinExpr {
                            span: DUMMY_SP,
                            op: swc_core::ecma::ast::BinaryOp::Add,
                            left: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: JsWord::from("foo"),
                                optional: false,
                            })),
                            right: Box::new(Expr::Ident(Ident {
                                span: DUMMY_SP,
                                sym: JsWord::from("bar"),
                                optional: false,
                            })),
                        }))),
                        ..Default::default()
                    })),
                },
                template_scope: 0,
            },
            r#"_createElementVNode("h1",{textContent:foo+bar})"#,
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
