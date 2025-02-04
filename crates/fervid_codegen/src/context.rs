use fervid_core::options::TransformAssetUrls;
use fervid_transform::BindingsHelper;
#[derive(Debug, Default)]
pub struct CodegenContext {
    pub bindings_helper: BindingsHelper,
    pub is_cache_disabled: bool,
    pub next_cache_index: u8,
    pub transform_asset_urls: TransformAssetUrls,
}

impl CodegenContext {
    pub fn with_bindings_helper(
        bindings_helper: BindingsHelper,
        transform_asset_urls: Option<TransformAssetUrls>,
    ) -> CodegenContext {
        CodegenContext {
            bindings_helper,
            transform_asset_urls: transform_asset_urls.unwrap_or_default(),
            ..Default::default()
        }
    }
}
