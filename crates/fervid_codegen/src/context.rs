use fervid_core::BindingsHelper;

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub bindings_helper: BindingsHelper,
}

impl CodegenContext {
    pub fn with_bindings_helper(bindings_helper: BindingsHelper) -> CodegenContext {
        CodegenContext { bindings_helper }
    }
}
