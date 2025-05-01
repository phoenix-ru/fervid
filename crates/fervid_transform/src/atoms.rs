//! Helper module to provide commonly used Vue words as static symbols (`FervidAtom`)

use fervid_core::{fervid_atom, FervidAtom};

lazy_static! {
    pub static ref VUE: FervidAtom = fervid_atom!("vue");

    // Options API atoms
    pub static ref DATA: FervidAtom = fervid_atom!("data");
    pub static ref SETUP: FervidAtom = fervid_atom!("setup");
    pub static ref PROPS: FervidAtom = fervid_atom!("props");
    pub static ref COMPUTED: FervidAtom = fervid_atom!("computed");
    pub static ref INJECT: FervidAtom = fervid_atom!("inject");
    pub static ref EMITS: FervidAtom = fervid_atom!("emits");
    pub static ref COMPONENTS: FervidAtom = fervid_atom!("components");
    pub static ref METHODS: FervidAtom = fervid_atom!("methods");
    pub static ref EXPOSE: FervidAtom = fervid_atom!("expose");
    pub static ref NAME: FervidAtom = fervid_atom!("name");
    pub static ref DIRECTIVES: FervidAtom = fervid_atom!("directives");

    // Composition API atoms
    // pub static ref COMPUTED: FervidAtom = fervid_atom!("computed");
    pub static ref DEFINE_EMITS: FervidAtom = fervid_atom!("defineEmits");
    pub static ref DEFINE_EXPOSE: FervidAtom = fervid_atom!("defineExpose");
    pub static ref DEFINE_MODEL: FervidAtom = fervid_atom!("defineModel");
    pub static ref DEFINE_OPTIONS: FervidAtom = fervid_atom!("defineOptions");
    pub static ref DEFINE_PROPS: FervidAtom = fervid_atom!("defineProps");
    pub static ref DEFINE_SLOTS: FervidAtom = fervid_atom!("defineSlots");
    pub static ref REACTIVE: FervidAtom = fervid_atom!("reactive");
    pub static ref REF: FervidAtom = fervid_atom!("ref");
    pub static ref TO_REF: FervidAtom = fervid_atom!("toRef");
    pub static ref WATCH: FervidAtom = fervid_atom!("watch");
    pub static ref WITH_DEFAULTS: FervidAtom = fervid_atom!("withDefaults");

    // Helper atoms
    pub static ref EMIT: FervidAtom = fervid_atom!("emit");
    pub static ref EMIT_HELPER: FervidAtom = fervid_atom!("__emit");
    pub static ref EXPOSE_HELPER: FervidAtom = fervid_atom!("__expose");
    pub static ref MERGE_MODELS_HELPER: FervidAtom = fervid_atom!("_mergeModels");
    pub static ref MODEL_VALUE: FervidAtom = fervid_atom!("modelValue");
    pub static ref PROPS_HELPER: FervidAtom = fervid_atom!("__props");
    pub static ref USE_MODEL_HELPER: FervidAtom = fervid_atom!("_useModel");
}
