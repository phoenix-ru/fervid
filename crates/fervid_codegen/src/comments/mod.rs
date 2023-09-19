use fervid_core::VueImports;
use swc_core::{
    common::Span,
    ecma::{
        ast::{CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, Str},
        atoms::{Atom, JsWord},
    },
};

use crate::context::CodegenContext;

impl CodegenContext {
    /// Generates `createCommentVNode("comment contents")`
    pub fn generate_comment_vnode(&mut self, comment: &str, span: Span) -> Expr {
        Expr::Call(CallExpr {
            span,
            // createCommentVNode
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_and_add_import_ident(VueImports::CreateCommentVNode),
                optional: false,
            }))),
            // "comment"
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    span,
                    value: JsWord::from(comment),
                    raw: Some(Atom::from(comment)),
                }))),
            }],
            type_args: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use swc_core::common::DUMMY_SP;

    use super::*;

    #[test]
    fn it_generates_comment() {
        test_out(
            "hi, this is some comment",
            r#"_createCommentVNode("hi, this is some comment")"#
        );
    }

    #[test]
    fn it_generates_quotes() {
        test_out(
            r#"In 'this' "string" `there` 'are" "multiple' `weird' 'quotes`"#,
            r#"_createCommentVNode("In 'this' \"string\" `there` 'are\" \"multiple' `weird' 'quotes`")"#
        );
    }

    fn test_out(input: &str, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_comment_vnode(&input, DUMMY_SP);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}