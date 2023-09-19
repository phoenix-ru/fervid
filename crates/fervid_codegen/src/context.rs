use fervid_core::VueImports;
use flagset::FlagSet;
use fxhash::FxHashMap as HashMap;
use swc_core::ecma::atoms::JsWord;

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub components: HashMap<String, JsWord>,
    pub directives: HashMap<String, JsWord>,
    pub used_imports: FlagSet<VueImports>,
}
