use fervid_core::{FervidAtom, IntoIdent, VueImports};
use swc_core::{
    common::Span,
    ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, Lit, Str},
};

use crate::context::CodegenContext;

impl CodegenContext {
    /// Generates `createCommentVNode("comment contents")`
    pub fn generate_comment_vnode(&mut self, comment: &str, span: Span, as_block: bool) -> Expr {
        // "comment"
        let comment_arg = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Str(Str {
                span,
                value: FervidAtom::from(comment),
                raw: None,
            }))),
        };

        let args = if as_block {
            let as_block_arg = ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::from(true)),
            };

            vec![comment_arg, as_block_arg]
        } else {
            vec![comment_arg]
        };
        Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            // createCommentVNode
            callee: Callee::Expr(Box::new(Expr::Ident(
                self.get_and_add_import_ident(VueImports::CreateCommentVNode)
                    .into_ident_spanned(span),
            ))),
            args,
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
            r#"_createCommentVNode("hi, this is some comment")"#,
        );
    }

    #[test]
    fn it_generates_quotes() {
        test_out(
            r#"In 'this' "string" `there` 'are" "multiple' `weird' 'quotes`"#,
            r#"_createCommentVNode("In 'this' \"string\" `there` 'are\" \"multiple' `weird' 'quotes`")"#,
        );
    }

    fn test_out(input: &str, expected: &str) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_comment_vnode(input, DUMMY_SP, false);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
