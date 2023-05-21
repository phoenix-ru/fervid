use swc_core::ecma::atoms::{JsWord, js_word};

lazy_static! {
    pub static ref DATA: JsWord = js_word!("data");
    pub static ref SETUP: JsWord = JsWord::from("setup");
    pub static ref PROPS: JsWord = JsWord::from("props");
    pub static ref COMPUTED: JsWord = JsWord::from("computed");
    pub static ref INJECT: JsWord = JsWord::from("inject");
    pub static ref EMITS: JsWord = JsWord::from("emits");
    pub static ref COMPONENTS: JsWord = JsWord::from("components");
    pub static ref METHODS: JsWord = JsWord::from("methods");
    pub static ref EXPOSE: JsWord = JsWord::from("expose");
    pub static ref NAME: JsWord = js_word!("name");
    pub static ref DIRECTIVES: JsWord = JsWord::from("directives");
}
