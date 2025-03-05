use fxhash::FxHashMap;

/// Asset URL tag configuration
/// Example: { "img": ["src"], "link": ["href"] }
pub type AssetURLTagConfig = FxHashMap<String, Vec<String>>;

#[derive(Debug, Clone)]
pub struct AssetURLOptions {
    /// Base path for rewriting URLs
    pub base: Option<String>,
    /// Whether to process absolute URLs
    pub include_absolute: bool,
    /// Tag-specific configuration
    pub tags: Option<AssetURLTagConfig>,
}

#[derive(Debug, Clone)]
pub enum TransformAssetUrls {
    Boolean(bool),
    Options(AssetURLOptions),
}

impl Default for TransformAssetUrls {
    fn default() -> Self {
        let mut tags = FxHashMap::default();
        tags.insert(
            "video".to_string(),
            vec!["src".to_string(), "poster".to_string()],
        );
        tags.insert("source".to_string(), vec!["src".to_string()]);
        tags.insert("img".to_string(), vec!["src".to_string()]);
        tags.insert(
            "image".to_string(),
            vec!["xlink:href".to_string(), "href".to_string()],
        );
        tags.insert(
            "use".to_string(),
            vec!["xlink:href".to_string(), "href".to_string()],
        );

        TransformAssetUrls::Options(AssetURLOptions {
            base: None,
            include_absolute: false,
            tags: Some(tags),
        })
    }
}
