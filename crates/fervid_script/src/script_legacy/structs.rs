use swc_core::ecma::atoms::JsWord;

#[derive(Debug, Default, PartialEq)]
pub struct ScriptLegacyVars {
    pub data: Vec<JsWord>,
    pub setup: Vec<JsWord>,
    pub props: Vec<JsWord>,
    pub inject: Vec<JsWord>,
    pub emits: Vec<JsWord>,
    pub components: Vec<JsWord>,
    pub computed: Vec<JsWord>,
    pub methods: Vec<JsWord>,
    pub expose: Vec<JsWord>,
    pub name: Option<JsWord>,
    pub directives: Vec<JsWord>,
}
