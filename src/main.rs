mod parser;

fn main() {
    let test = include_str!("./test/input.vue");
    let res = parser::parse_sfc(test).unwrap();
    println!("Result: {:?}", res.1);
    println!("Remaining: {:?}", res.0);

    println!();
    let test = "<self-closing-example />";
    let res = parser::parse_element_node(test).unwrap();
    println!("Result: {:?}", res.1);

    println!();
    let test = "<div><template v-slot:[dynamicSlot]>hello</template></div>";
    let res = parser::parse_element_node(test).unwrap();
    println!("Result: {:?}", res.1);
}
