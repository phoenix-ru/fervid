use fervid_core::options::{
    AssetURLOptions as CoreAssetURLOptions, TransformAssetUrls as CoreTransformAssetUrls,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;

#[napi(object)]
#[derive(Clone, Debug)]
pub struct NapiAssetUrlOptions {
    pub base: Option<String>,
    pub include_absolute: Option<bool>,
    pub tags: Option<HashMap<String, Vec<String>>>,
}

impl From<NapiAssetUrlOptions> for CoreTransformAssetUrls {
    fn from(value: NapiAssetUrlOptions) -> Self {
        CoreTransformAssetUrls::Options(CoreAssetURLOptions {
            base: value.base,
            include_absolute: value.include_absolute.unwrap_or(false),
            tags: value.tags,
        })
    }
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
