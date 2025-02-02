use std::collections::HashMap;

/// Asset URL tag configuration
/// Example: { "img": ["src"], "link": ["href"] }
pub type AssetURLTagConfig = HashMap<String, Vec<String>>;

#[derive(Debug, Clone)]
pub enum TransformAssetUrls {
    /// Enable/disable asset URL transformation
    Boolean(bool),
    /// Detailed configuration for asset URL transformation
    Config(AssetURLOptions),
    /// Direct tag configuration
    TagConfig(AssetURLTagConfig),
}

#[derive(Debug, Clone)]
pub struct AssetURLOptions {
    /// Base path for rewriting URLs
    pub base: Option<String>,
    /// Whether to process absolute URLs
    pub include_absolute: bool,
    /// Tag-specific configuration
    pub tags: Option<AssetURLTagConfig>,
}

impl Default for TransformAssetUrls {
    fn default() -> Self {
        let mut tags = HashMap::new();
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

        TransformAssetUrls::Config(AssetURLOptions {
            base: None,
            include_absolute: false,
            tags: Some(tags),
        })
    }
}
