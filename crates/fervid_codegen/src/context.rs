use fervid_transform::BindingsHelper;
use fervid_core::options::TransformAssetUrls;
#[derive(Debug, Default)]
pub struct CodegenContext {
    pub bindings_helper: BindingsHelper,
    pub is_cache_disabled: bool,
    pub next_cache_index: u8,
    pub transform_asset_urls: TransformAssetUrls,
}

impl CodegenContext {
    pub fn with_bindings_helper(bindings_helper: BindingsHelper) -> CodegenContext {
        CodegenContext {
            bindings_helper,
            ..Default::default()
        }
    }
}
