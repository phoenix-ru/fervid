extern crate swc_ecma_parser;
extern crate swc_common;
extern crate swc_core;
extern crate swc_ecma_codegen;
use std::time::Instant;

use crate::compiler::codegen::compile_ast;
use crate::parser::structs::{StartingTag, Node, ElementNode};
use crate::parser::{html_utils::ElementKind, attributes::HtmlAttribute};
use crate::analyzer::ast_optimizer;

mod parser;
mod analyzer;
mod compiler;

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
    println!("SWC result: {}", swc_result.unwrap().trim().trim_end_matches(";"));
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
    let test = include_str!("./test/input.vue");

    // Parse
    let res = parser::core::parse_sfc(test).unwrap();

    // Optimize
    let mut ast = res.1;
    ast_optimizer::optimize_ast(&mut ast);

    // Analyze scopes
    let mut scope_helper = analyzer::scope::ScopeHelper::default();
    scope_helper.transform_and_record_ast(&mut ast);

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
    println!(
        "{}",
        compile_ast(&ast, scope_helper).unwrap()
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
                    Node::DynamicExpression { value: "testRef", template_scope: 0 },
                    Node::TextNode("yes yes"),
                    // Just element
                    Node::ElementNode(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "i",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: ElementKind::Normal
                        },
                        children: vec![
                            Node::TextNode("italics, mm"),
                            Node::DynamicExpression { value: "hey", template_scope: 0 }
                        ],
                        template_scope: 0
                    }),
                    // Component
                    Node::ElementNode(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "CustomComponent",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: ElementKind::Normal // is this needed?
                        },
                        children: vec![
                            Node::TextNode("italics, mm"),
                            Node::DynamicExpression { value: "hey", template_scope: 0 }
                        ],
                        template_scope: 0
                    }),
                    Node::TextNode("end of span node")
                ],
                template_scope: 0
            })
        ],
        template_scope: 0
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
        ],
        template_scope: 0
    });

    println!("\n[Synthetic Compile Result]\n");
    println!(
        "{}",
        compile_ast(&[template, script], Default::default()).unwrap()
    );
}
