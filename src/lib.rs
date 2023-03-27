use std::rc::Rc;

use swc_core::ecma::ast::{Ident, MemberExpr};
use swc_core::ecma::atoms::JsWord;
use swc_common::{BytePos, SourceMap, sync::Lrc};
use swc_ecma_parser::Parser;
use swc_ecma_parser::{lexer::Lexer, Syntax, StringInput};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_codegen::Node as CodegenNode;

mod analyzer;
mod compiler;
mod parser;
mod templates;

pub use compiler::codegen::compile_ast;
pub use parser::parse_sfc;
pub use templates::ast_optimizer::optimize_ast;

#[allow(dead_code)]
pub fn test_swc_transform(source_code: &str) {
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
    match parser.parse_expr() {
        Ok(mut v) => {
            let mut folder: TransformVisitor = Default::default();
            v.visit_mut_with(&mut folder);
            // println!("{:?}", v);

            let cm: Lrc<SourceMap> = Default::default();
            let mut buff: Rc<Vec<u8>> = Rc::new(Vec::with_capacity(source_code.len() * 2));
            let buff2: &mut Vec<u8> = Rc::get_mut(&mut buff).unwrap();
            let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", buff2, None);

            let mut emitter = Emitter {
                cfg: swc_ecma_codegen::Config { target: swc_core::ecma::ast::EsVersion::default(), ascii_only: false, minify: false, omit_last_semi: true },
                comments: None,
                wr: writer,
                cm
            };

            let res = v.emit_with(&mut emitter);

            if let Ok(_) = res {
                // println!("{}", std::str::from_utf8(&buff).unwrap())
            }
        },
        Err(e) => {
            eprintln!("{:?}", e)
        }
    }
}

#[derive(Default)]
pub struct TransformVisitor;

impl VisitMut for TransformVisitor {
    // fn visit_mut_ident(self: &mut Self, n: &mut Ident) {
    //     // println!("Member count: {}", self.);
    //     if !self.in_member_expr || !self.has_visited {
    //         n.sym = JsWord::from("barssss");
    //         self.has_visited = true;
    //     }
    // }

    fn visit_mut_member_expr(self: &mut Self, n: &mut MemberExpr) {
        if n.obj.is_ident() {
            // println!("Yes");
            let old_ident = n.obj.clone().expect_ident();
            n.obj = Box::from(MemberExpr {
                obj: Box::from(Ident::new(JsWord::from("_ctx"), swc_common::Span { lo: n.span.lo, hi: n.span.hi, ctxt: n.span.ctxt })),
                prop: swc_core::ecma::ast::MemberProp::Ident(old_ident),
                span: n.span
            })
        } else {
            n.visit_mut_children_with(self);
        }
    }
}
