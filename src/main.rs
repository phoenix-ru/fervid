use crate::compiler::codegen::compile_sfc;
use crate::parser::{Node, StartingTag, html_utils::ElementKind, attributes::HtmlAttribute};

mod parser;
mod analyzer;
mod compiler;

fn main() {
    let test = include_str!("./test/input.vue");
    let res = parser::parse_sfc(test).unwrap();
    println!("Result: {:?}", res.1);
    println!("Remaining: {:?}", res.0);

    println!();
    println!("SFC blocks length: {}", res.1.len());

    // Codegen testing
    let template = Node::ElementNode {
        starting_tag: StartingTag {
            tag_name: "template",
            attributes: vec![],
            is_self_closing: false,
            kind: ElementKind::Normal
        },
        children: vec![
            Node::ElementNode {
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
                    Node::ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "i",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: ElementKind::Normal
                        },
                        children: vec![Node::TextNode("italics, mm"), Node::DynamicExpression("hey")]
                    },
                    // Component
                    Node::ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "CustomComponent",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: ElementKind::Normal // is this needed?
                        },
                        children: vec![Node::TextNode("italics, mm"), Node::DynamicExpression("hey")]
                    },
                    Node::TextNode("end of span node")
                ]
            }
        ]
    };
    let script = Node::ElementNode {
        starting_tag: StartingTag {
            tag_name: "script",
            attributes: vec![HtmlAttribute::Regular { name: "lang", value: "js" }],
            is_self_closing: false,
            kind: ElementKind::RawText
        },
        children: vec![
            Node::TextNode("export default {{\n  name: 'TestComponent'\n}}")
        ]
    };
    println!(
        "{}",
        compile_sfc(&[template, script]).unwrap()
    );

    // println!();
    // let test = "<self-closing-example />";
    // let res = parser::parse_element_node(test).unwrap();
    // println!("Result: {:?}", res.1);

    // println!();
    // let test = "<div><template v-slot:[dynamicSlot]>hello</template></div>";
    // let res = parser::parse_element_node(test).unwrap();
    // println!("Result: {:?}", res.1);
}
