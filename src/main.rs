mod parser;

fn main() {
    let test = include_bytes!("./test/input.vue");
    let res = parser::parse_starting_tag(test).unwrap();
    println!("Result: {:?}", res.1);
    println!("Remaining: {:?}", std::str::from_utf8(res.0).unwrap());
}
