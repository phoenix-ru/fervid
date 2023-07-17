extern crate swc_core;
extern crate swc_ecma_codegen;
extern crate swc_ecma_parser;
use std::time::Instant;

use fervid::{compile_sfc, SfcBlock, SfcScriptBlock};
use fervid_core::{AttributeOrBinding, ElementNode, Interpolation, Node, StartingTag};

use fervid::analyzer::ast_optimizer;
use fervid::analyzer::scope::ScopeHelper;
use fervid::parser;
use swc_core::{ecma::{ast::{Expr, Ident}, atoms::JsWord}, common::DUMMY_SP};

fn main() {
    let n = Instant::now();
    test_real_compilation();
    println!("Time took: {:?}", n.elapsed());

    let n = Instant::now();
    test_synthetic_compilation();
    println!("Time took: {:?}", n.elapsed());

    println!();

    let n = Instant::now();
    let swc_result = fervid::test_swc_transform("[a, b, c, { d }]");
    println!(
        "SWC result: {}",
        swc_result.unwrap().trim().trim_end_matches(";")
    );
    println!("Time took for transform: {:?}", n.elapsed());

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

    // Optimize
    ast_optimizer::optimize_template(template_block);

    // Analyze scopes
    let mut scope_helper = ScopeHelper::default();
    scope_helper.transform_and_record_ast(&mut template_block.roots);

    // TODO Implement scope helper walking for the Expr'd nodes

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
    println!("{}", compile_sfc(sfc_blocks, scope_helper).unwrap());
}

#[allow(dead_code)]
fn test_synthetic_compilation() {
    let blocks = vec![
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
                    }),
                    Node::Text("end of span node"),
                ],
                template_scope: 0,
            })],
        }),
        SfcBlock::Script(SfcScriptBlock {
            lang: "js",
            content: "export default {\n  name: 'TestComponent'\n}",
            is_setup: false,
        }),
    ];

    println!("\n[Synthetic Compile Result]\n");
    println!("{}", compile_sfc(blocks, Default::default()).unwrap());
}
