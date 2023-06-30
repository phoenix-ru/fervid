use flagset::FlagSet;
use fxhash::FxHashMap as HashMap;
use swc_core::ecma::atoms::JsWord;

use crate::{transform::MockScopeHelper, imports::VueImports};

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub components: HashMap<String, JsWord>,
    pub used_imports: FlagSet<VueImports>,
    pub scope_helper: MockScopeHelper
}
