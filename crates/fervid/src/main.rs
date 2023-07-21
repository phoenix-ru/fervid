extern crate swc_core;
extern crate swc_ecma_codegen;
extern crate swc_ecma_parser;
use std::time::Instant;

use fervid::{SfcBlock, SfcScriptBlock};
use fervid_core::{AttributeOrBinding, ElementNode, Interpolation, Node, StartingTag, ElementKind};

use fervid::parser;
use fervid_transform::template::transform_and_record_template;
use swc_core::{ecma::{ast::{Expr, Ident}, atoms::JsWord}, common::DUMMY_SP};

fn main() {
    let n = Instant::now();
    test_real_compilation();
    println!("Time took: {:?}", n.elapsed());

    let n = Instant::now();
    test_synthetic_compilation();
    println!("Time took: {:?}", n.elapsed());
}

#[allow(dead_code)]
fn test_real_compilation() {
    let test = include_str!("../benches/fixtures/input.vue");

    // Parse
    let res = parser::core::parse_sfc(test).unwrap();

    assert!(res.0.trim().len() == 0, "Input was not fully consumed");

    // Find template block
    let mut sfc_blocks = res.1;
    let template_block = sfc_blocks.iter_mut().find_map(|block| match block {
        fervid::SfcBlock::Template(template_block) => Some(template_block),
        _ => None,
    });
    let Some(template_block) = template_block else {
        panic!("Test component has no template block");
    };

    let mut scope_helper = fervid_transform::template::ScopeHelper::default();
    transform_and_record_template(template_block, &mut scope_helper);

    let mut ctx = fervid_codegen::CodegenContext::default();
    let template_expr = ctx.generate_sfc_template(&template_block);

    // TODO
    let script = swc_core::ecma::ast::Module { span: DUMMY_SP, body: vec![], shebang: None };
    let sfc_module = ctx.generate_module(template_expr, script);

    let compiled_code = fervid_codegen::CodegenContext::stringify(&sfc_module, false);

    #[cfg(feature = "dbg_print")]
    {
        println!("Result: {:#?}", ast);
        println!("Remaining: {:?}", res.0);

        println!();
        println!("SFC blocks length: {}", ast.len());

        println!();
        println!("Scopes: {:#?}", scope_helper);
    }

    // Real codegen
    println!("\n[Real File Compile Result]");
    println!("{compiled_code}");
    // println!("{}", compile_sfc(sfc_blocks, scope_helper).unwrap());
}

#[allow(dead_code)]
fn test_synthetic_compilation() {
    let mut blocks = vec![
        SfcBlock::Template(fervid::SfcTemplateBlock {
            lang: "html",
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "span",
                    attributes: vec![AttributeOrBinding::RegularAttribute {
                        name: "class",
                        value: "yes",
                    }],
                    directives: None,
                },
                children: vec![
                    Node::Text("Hello world"),
                    Node::Interpolation(Interpolation {
                        value: Box::new(Expr::Ident(Ident {
                            span: DUMMY_SP,
                            sym: JsWord::from("testRef"),
                            optional: false,
                        })),
                        template_scope: 0,
                        patch_flag: true
                    }),
                    Node::Text("yes yes"),
                    // Just element
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "i",
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![
                            Node::Text("italics, mm"),
                            Node::Interpolation(Interpolation {
                                value: Box::new(Expr::Ident(Ident {
                                    span: DUMMY_SP,
                                    sym: JsWord::from("hey"),
                                    optional: false,
                                })),
                                template_scope: 0,
                                patch_flag: true
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Element
                    }),
                    // Component
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "CustomComponent",
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![
                            Node::Text("italics, mm"),
                            Node::Interpolation(Interpolation {
                                value: Box::new(Expr::Ident(Ident {
                                    span: DUMMY_SP,
                                    sym: JsWord::from("hey"),
                                    optional: false,
                                })),
                                template_scope: 0,
                                patch_flag: true
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Component
                    }),
                    Node::Text("end of span node"),
                ],
                template_scope: 0,
                kind: ElementKind::Element
            })],
        }),
        SfcBlock::Script(SfcScriptBlock {
            lang: "js",
            content: "export default {\n  name: 'TestComponent'\n}",
            is_setup: false,
        }),
    ];

    let template_block = blocks.iter_mut().find_map(|block| match block {
        fervid::SfcBlock::Template(template_block) => Some(template_block),
        _ => None,
    });
    let Some(template_block) = template_block else {
        panic!("Test component has no template block");
    };

    let mut scope_helper = fervid_transform::template::ScopeHelper::default();
    transform_and_record_template(template_block, &mut scope_helper);

    let mut ctx = fervid_codegen::CodegenContext::default();
    let template_expr = ctx.generate_sfc_template(&template_block);

    // TODO
    let script = swc_core::ecma::ast::Module { span: DUMMY_SP, body: vec![], shebang: None };
    let sfc_module = ctx.generate_module(template_expr, script);

    let compiled_code = fervid_codegen::CodegenContext::stringify(&sfc_module, false);

    println!("\n[Synthetic Compile Result]\n");
    println!("{compiled_code}");
}
