use fervid_core::options::{
    AssetURLOptions as CoreAssetURLOptions, TransformAssetUrls as CoreTransformAssetUrls,
};
use fxhash::FxHashMap;
use napi_derive::napi;

#[napi(object)]
#[derive(Clone, Debug)]
pub struct NapiAssetUrlOptions {
    pub base: Option<String>,
    pub include_absolute: Option<bool>,
    pub tags: Option<FxHashMap<String, Vec<String>>>,
}

impl From<&NapiAssetUrlOptions> for CoreTransformAssetUrls {
    fn from(value: &NapiAssetUrlOptions) -> Self {
        CoreTransformAssetUrls::Options(CoreAssetURLOptions {
            base: value.base.clone(),
            include_absolute: value.include_absolute.unwrap_or(false),
            tags: value.tags.clone(),
        })
    }
}
