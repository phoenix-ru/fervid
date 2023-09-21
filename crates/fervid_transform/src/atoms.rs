//! Helper module to provide commonly used Vue words as static symbols (`JsWord`)

use swc_core::ecma::atoms::{JsWord, js_word};

lazy_static! {
    pub static ref VUE: JsWord = JsWord::from("vue");

    // Options API atoms
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

    // Composition API atoms
    // pub static ref COMPUTED: JsWord = JsWord::from("computed");
    pub static ref DEFINE_EMITS: JsWord = JsWord::from("defineEmits");
    pub static ref DEFINE_EXPOSE: JsWord = JsWord::from("defineExpose");
    pub static ref DEFINE_MODEL: JsWord = JsWord::from("defineModel");
    pub static ref DEFINE_PROPS: JsWord = JsWord::from("defineProps");
    pub static ref REACTIVE: JsWord = JsWord::from("reactive");
    pub static ref REF: JsWord = JsWord::from("ref");

    // Helper atoms
    pub static ref EMIT: JsWord = JsWord::from("emit");
    pub static ref EMIT_HELPER: JsWord = JsWord::from("__emit");
    pub static ref EXPOSE_HELPER: JsWord = JsWord::from("__expose");
    pub static ref MERGE_MODELS_HELPER: JsWord = JsWord::from("_mergeModels");
    pub static ref MODEL_VALUE: JsWord = JsWord::from("modelValue");
    pub static ref PROPS_HELPER: JsWord = JsWord::from("__props");
    pub static ref USE_MODEL_HELPER: JsWord = JsWord::from("_useModel");
}
