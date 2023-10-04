use fervid_core::{VueImports, FervidAtom};
use flagset::FlagSet;
use fxhash::FxHashMap as HashMap;
use swc_core::ecma::atoms::JsWord;

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub components: HashMap<FervidAtom, JsWord>,
    pub directives: HashMap<FervidAtom, JsWord>,
    pub used_imports: FlagSet<VueImports>,
}
