extern crate swc_core;
extern crate swc_ecma_codegen;
extern crate swc_ecma_parser;
use std::time::Instant;

use fervid::{parser::ecma::parse_js_module, SfcScriptBlock};
use fervid_core::{
    AttributeOrBinding, ElementKind, ElementNode, Interpolation, Node, SfcDescriptor, StartingTag,
};

use fervid::parser;
use fervid_transform::{template::transform_and_record_template, script::transform_and_record_scripts};
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{Expr, Ident},
        atoms::JsWord,
    },
};

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
    let (remaining_input, sfc) = parser::core::parse_sfc(test).unwrap();

    assert!(
        remaining_input.trim().len() == 0,
        "Input was not fully consumed"
    );

    // Find template block
    let mut template_block = sfc.template;
    let Some(ref mut template_block) = template_block else {
        panic!("Test component has no template block");
    };

    let mut scope_helper = fervid_transform::template::ScopeHelper::default();
    let module = transform_and_record_scripts(sfc.script_setup, sfc.script_legacy, &mut scope_helper);
    transform_and_record_template(template_block, &mut scope_helper);

    let mut ctx = fervid_codegen::CodegenContext::default();
    let template_expr = ctx.generate_sfc_template(&template_block);

    // TODO
    let sfc_module = ctx.generate_module(template_expr, module);

    let compiled_code = fervid_codegen::CodegenContext::stringify(test, &sfc_module, false);

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
    let sfc = SfcDescriptor {
        template: Some(fervid::SfcTemplateBlock {
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
                        patch_flag: true,
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
                                patch_flag: true,
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Element,
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
                                patch_flag: true,
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Component,
                    }),
                    Node::Text("end of span node"),
                ],
                template_scope: 0,
                kind: ElementKind::Element,
            })],
        }),
        script_legacy: Some(SfcScriptBlock {
            lang: "js",
            content: js_module("export default {\n  name: 'TestComponent'\n}"),
            is_setup: false,
        }),
        script_setup: None,
        styles: vec![],
        custom_blocks: vec![],
    };

    let mut template_block = sfc.template;
    let Some(ref mut template_block) = template_block else {
        panic!("Test component has no template block");
    };

    let mut scope_helper = fervid_transform::template::ScopeHelper::default();
    transform_and_record_template(template_block, &mut scope_helper);

    let mut ctx = fervid_codegen::CodegenContext::default();
    let template_expr = ctx.generate_sfc_template(&template_block);

    // TODO
    let script = swc_core::ecma::ast::Module {
        span: DUMMY_SP,
        body: vec![],
        shebang: None,
    };
    let sfc_module = ctx.generate_module(template_expr, script);

    let compiled_code = fervid_codegen::CodegenContext::stringify("", &sfc_module, false);

    println!("\n[Synthetic Compile Result]\n");
    println!("{compiled_code}");
}

fn js_module(raw: &str) -> Box<swc_core::ecma::ast::Module> {
    parse_js_module(raw, 0, 0).unwrap().into()
}
