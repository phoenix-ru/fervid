use crate::compiler::codegen::compile_template;
use crate::parser::{Node, StartingTag, html_utils::ElementKind};

mod parser;
mod compiler;

fn main() {
    let test = include_str!("./test/input.vue");
    let res = parser::parse_sfc(test).unwrap();
    println!("Result: {:?}", res.1);
    println!("Remaining: {:?}", res.0);

    println!();
    println!("SFC blocks length: {}", res.1.len());

    // Codegen testing
    println!(
        "{}",
        compile_template(Node::ElementNode {
            starting_tag: StartingTag {
            tag_name: "span",
            attributes: vec![],
            is_self_closing: false,
            kind: ElementKind::Normal
            },
            children: vec![Node::TextNode("Hello world")]
        })
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
