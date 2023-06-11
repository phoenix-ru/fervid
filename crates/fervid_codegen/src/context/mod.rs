use fxhash::FxHashMap as HashMap;
use swc_core::ecma::atoms::JsWord;

use crate::transform::MockScopeHelper;

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub components: HashMap<String, JsWord>,
    pub used_imports: u64,
    pub scope_helper: MockScopeHelper
}
