macro_rules! input {
    ($name: literal) => {
        ($name, include_str!(concat!("./", $name)))
    };
}

pub const FIXTURES: [(&str, &str); 3] = [
    input!("input.vue"),
    input!("ElTable.vue"),
    input!("TodoApp.vue"),
];
