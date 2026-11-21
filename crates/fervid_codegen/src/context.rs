use fervid_transform::BindingsHelper;

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub bindings_helper: BindingsHelper,
    pub next_cache_index: u32,
}

impl CodegenContext {
    pub fn with_bindings_helper(bindings_helper: BindingsHelper) -> CodegenContext {
        CodegenContext {
            bindings_helper,
            ..Default::default()
        }
    }
}
