extern crate lazy_static;

use std::rc::Rc;

use swc_common::Span;
use swc_core::ecma::ast::{Ident, MemberExpr, MemberProp, Expr};
use swc_core::ecma::atoms::JsWord;
use swc_common::{BytePos, SourceMap, sync::Lrc};
use swc_ecma_parser::Parser;
use swc_ecma_parser::error::Error;
use swc_ecma_parser::{lexer::Lexer, Syntax, StringInput};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_codegen::Node as CodegenNode;

mod analyzer;
mod compiler;
mod parser;

pub use analyzer::ast_optimizer::optimize_ast;
pub use compiler::codegen::compile_ast;
pub use parser::core::parse_sfc;

#[allow(dead_code)]
pub fn test_swc_transform(source_code: &str) -> Result<Rc<Vec<u8>>, Error> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(source_code, BytePos(100), BytePos(1000)),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    // println!();
    // TODO The use of `parse_expr` vs `parse_stmt` matters
    // For v-for it's best to use `parse_stmt`, but for `v-slot` you need to use `parse_expr`
    match parser.parse_expr() {
        Ok(mut v) => {
            let mut folder: TransformVisitor = Default::default();
            // println!("{:#?}", v);
            // println!("SWC is expression: {}", v.is_expr());
            // if v.is_expr() {
            //     println!("SWC is identifier: {}", v.as_expr().unwrap().expr.is_ident());
            // }
            v.visit_mut_with(&mut folder);

            let cm: Lrc<SourceMap> = Default::default();
            let mut buff: Rc<Vec<u8>> = Rc::new(Vec::with_capacity(source_code.len() * 2));
            let buff2: &mut Vec<u8> = Rc::get_mut(&mut buff).unwrap();
            let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", buff2, None);

            let mut emitter = Emitter {
                cfg: swc_ecma_codegen::Config {
                    target: Default::default(),
                    ascii_only: false,
                    minify: false,
                    omit_last_semi: false
                },
                comments: None,
                wr: writer,
                cm
            };

            let res = v.emit_with(&mut emitter);

            if let Ok(_) = res {
                // println!("{}", std::str::from_utf8(&buff).unwrap())
                // return Ok(buff);
            }

            Ok(buff)
        },
        Err(e) => {
            eprintln!("{:?}", e);
            return Err(e);
        }
    }
}

#[derive(Default)]
pub struct TransformVisitor;

impl VisitMut for TransformVisitor {
    fn visit_mut_ident(&mut self, n: &mut Ident) {
        let mut new_ident = String::with_capacity(5 + n.sym.len());
        new_ident.push_str("_ctx.");
        new_ident.push_str(&n.sym);
        n.sym = JsWord::from(new_ident);
    }

    fn visit_mut_member_expr(self: &mut Self, n: &mut MemberExpr) {
        if let Some(old_ident) = n.obj.as_ident() {
            n.obj = prefix_ident(old_ident, JsWord::from("_ctx"), n.span);
        } else {
            n.visit_mut_children_with(self);
        }
    }

    // Is this needed ?
    fn visit_mut_bin_expr(&mut self, n: &mut swc_core::ecma::ast::BinExpr) {
        n.right.visit_mut_children_with(self)
    }
}

fn prefix_ident(old_ident: &Ident, prefix_with: JsWord, span: Span) -> Box<Expr> {
    Box::from(MemberExpr {
        obj: Box::from(Ident::new(prefix_with, span)),
        prop: MemberProp::Ident(old_ident.to_owned()),
        span
    })
}
