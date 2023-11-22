use fervid_core::BindingsHelper;

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub bindings_helper: BindingsHelper,
    pub is_cache_disabled: bool,
    pub next_cache_index: u8
}

impl CodegenContext {
    pub fn with_bindings_helper(bindings_helper: BindingsHelper) -> CodegenContext {
        CodegenContext {
            bindings_helper,
            ..Default::default()
        }
    }
}
