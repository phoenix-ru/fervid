extern crate swc_ecma_parser;
extern crate swc_common;
extern crate swc_core;
extern crate swc_ecma_codegen;
use std::rc::Rc;
use std::time::Instant;

use swc_core::ecma::ast::{Ident, MemberExpr};
use swc_core::ecma::atoms::JsWord;
use swc_common::{BytePos, SourceMap, sync::Lrc};
use swc_ecma_parser::Parser;
use swc_ecma_parser::{lexer::Lexer, Syntax, StringInput};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};

use crate::compiler::codegen::compile_sfc;
use crate::parser::structs::{StartingTag, Node, ElementNode};
use crate::parser::{html_utils::ElementKind, attributes::HtmlAttribute};
use crate::swc_ecma_codegen::Node as CodegenNode;
use crate::templates::ast_optimizer;

mod parser;
mod analyzer;
mod compiler;
mod templates;

fn main() {
    let n = Instant::now();
    test_real_compilation();
    println!("Time took: {:?}", n.elapsed());

    let n = Instant::now();
    test_synthetic_compilation();
    println!("Time took: {:?}", n.elapsed());

    // let n = Instant::now();
    // test_swc_transform();
    // println!("Time took for transform: {:?}", n.elapsed());

    // println!("", swc_ecma_parser::parse_file_as_expr(fm, syntax, target, comments, recovered_errors));

    // println!();
    // let test = "<self-closing-example />";
    // let res = parser::parse_element_node(test).unwrap();
    // println!("Result: {:?}", res.1);

    // println!();
    // let test = "<div><template v-slot:[dynamicSlot]>hello</template></div>";
    // let res = parser::parse_element_node(test).unwrap();
    // println!("Result: {:?}", res.1);
}

#[allow(dead_code)]
fn test_real_compilation() {
    let test = include_str!("./test/input.vue");
    let mut res = parser::parse_sfc(test).unwrap();
    let optimized_ast = ast_optimizer::optimize_ast(&mut res.1);

    #[cfg(dbg_print)]
    {
        println!("Result: {:#?}", optimized_ast);
        println!("Remaining: {:?}", res.0);
    
        println!();
        println!("SFC blocks length: {}", optimized_ast.len());
    }

    // Real codegen
    println!("\n[Real File Compile Result]");
    println!(
        "{}",
        compile_sfc(optimized_ast).unwrap()
    );
}

#[allow(dead_code)]
fn test_synthetic_compilation() {
    // Codegen testing
    let template = Node::ElementNode(ElementNode {
        starting_tag: StartingTag {
            tag_name: "template",
            attributes: vec![],
            is_self_closing: false,
            kind: ElementKind::Normal
        },
        children: vec![
            Node::ElementNode(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "span",
                    attributes: vec![HtmlAttribute::Regular { name: "class", value: "yes" }],
                    is_self_closing: false,
                    kind: ElementKind::Normal
                },
                children: vec![
                    Node::TextNode("Hello world"),
                    Node::DynamicExpression("testRef"),
                    Node::TextNode("yes yes"),
                    // Just element
                    Node::ElementNode(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "i",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: ElementKind::Normal
                        },
                        children: vec![Node::TextNode("italics, mm"), Node::DynamicExpression("hey")]
                    }),
                    // Component
                    Node::ElementNode(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "CustomComponent",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: ElementKind::Normal // is this needed?
                        },
                        children: vec![Node::TextNode("italics, mm"), Node::DynamicExpression("hey")]
                    }),
                    Node::TextNode("end of span node")
                ]
            })
        ]
    });
    let script = Node::ElementNode(ElementNode {
        starting_tag: StartingTag {
            tag_name: "script",
            attributes: vec![HtmlAttribute::Regular { name: "lang", value: "js" }],
            is_self_closing: false,
            kind: ElementKind::RawText
        },
        children: vec![
            Node::TextNode("export default {\n  name: 'TestComponent'\n}")
        ]
    });

    println!("\n[Synthetic Compile Result]\n");
    println!(
        "{}",
        compile_sfc(&[template, script]).unwrap()
    );
}

#[allow(dead_code)]
fn test_swc_transform() {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new("foo.bar.baz[test.keks]", BytePos(100), BytePos(1000)),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    println!();
    match parser.parse_expr() {
        Ok(mut v) => {
            let mut folder: TransformVisitor = Default::default();
            v.visit_mut_with(&mut folder);
            println!("{:?}", v);

            let cm: Lrc<SourceMap> = Default::default();
            let mut buff: Rc<Vec<u8>> = Rc::new(Vec::new());
            let buff2: &mut Vec<u8> = Rc::get_mut(&mut buff).unwrap();
            let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", buff2, None);

            let mut emitter = Emitter {
                cfg: swc_ecma_codegen::Config { target: swc_core::ecma::ast::EsVersion::Es2022, ascii_only: false, minify: false, omit_last_semi: true },
                comments: None,
                wr: writer,
                cm
            };

            let res = v.emit_with(&mut emitter);

            if let Ok(_) = res {
                println!("{}", std::str::from_utf8(&buff).unwrap())
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
            println!("Yes");
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
