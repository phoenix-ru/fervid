use fervid_core::{
    AssetURLOptions, AttributeOrBinding, ElementNode, FervidAtom, Node, StrOrExpr,
    TransformAssetUrls,
};
use fxhash::FxHashMap;
use std::path::{Path, PathBuf};
use swc_core::common::Span;

pub fn transform_asset_urls(
    node: &mut Node,
    transform_option: &TransformAssetUrls,
    filename: &str,
) {
    // TODO add default options TransformAssetUrls with enum add boolean type
    let options = match transform_option {
        TransformAssetUrls::Boolean(false) => return,
        TransformAssetUrls::Boolean(true) => AssetURLOptions {
            base: None,
            include_absolute: false,
            tags: get_default_tags(),
        },
        TransformAssetUrls::Options(ref opts) => opts.clone(),
    };

    transform_asset_urls_impl(node, &options, filename);
}

/// 内部实现函数，使用 AssetURLOptions
fn transform_asset_urls_impl(node: &mut Node, options: &AssetURLOptions, filename: &str) {
    match node {
        Node::Element(element) => transform_element_asset_urls(element, options, filename),
        _ => {
            if let Some(children) = get_children_from_node(node) {
                for child in children {
                    transform_asset_urls_impl(child, options, filename);
                }
            }
        }
    }
}

fn get_children_from_node(node: &mut Node) -> Option<Vec<&mut Node>> {
    match node {
        Node::Element(element) => Some(element.children.iter_mut().collect()),
        _ => None,
    }
}

fn transform_element_asset_urls(
    element: &mut ElementNode,
    options: &AssetURLOptions,
    filename: &str,
) {
    let tags = &options.tags;
    if let Some(attrs) = tags.get(&element.starting_tag.tag_name.to_string()) {
        for attr_name in attrs {
            transform_element_attribute(element, attr_name, options, filename);
        }
    }

    for child in element.children.iter_mut() {
        transform_asset_urls_impl(child, options, filename);
    }
}

fn transform_element_attribute(
    element: &mut ElementNode,
    attr_name: &str,
    options: &AssetURLOptions,
    filename: &str,
) {
    let attr_atom = FervidAtom::from(attr_name);

    for attr in &mut element.starting_tag.attributes {
        match attr {
            AttributeOrBinding::RegularAttribute { name, value, .. } if *name == attr_atom => {
                *value = FervidAtom::from(transform_url(&value.to_string(), options, filename));
            }
            AttributeOrBinding::VBind(binding) => {
                if let Some(arg) = &binding.argument {
                    match arg {
                        StrOrExpr::Str(name) if *name == attr_atom => {
                            // TODO dynamic import handle this ?
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn transform_url(url_str: &str, options: &AssetURLOptions, _filename: &str) -> String {
    if url_str.trim().is_empty() {
        return url_str.to_string();
    }

    if url_str.starts_with("http://")
        || url_str.starts_with("https://")
        || url_str.starts_with("//")
        || url_str.starts_with("data:")
        || url_str.starts_with("blob:")
    {
        return url_str.to_string();
    }

    let path = Path::new(url_str);
    if path.is_absolute() {
        if options.include_absolute {
            return url_str.to_string();
        }

        if let Some(base) = &options.base {
            let mut path_buf = PathBuf::from(base);
            let path_str = path.to_string_lossy();
            let clean_url = path_str.trim_start_matches('/');
            path_buf.push(clean_url);

            let result_str = path_buf.to_string_lossy().to_string();
            if !Path::new(&result_str).is_absolute() {
                let mut new_path = PathBuf::from("/");
                new_path.push(result_str);
                return new_path.to_string_lossy().to_string();
            }
            return result_str;
        }

        return url_str.to_string();
    }

    if let Some(base) = &options.base {
        let base_path = Path::new(base);

        let clean_url = if url_str.starts_with("./") {
            Path::new(url_str)
                .strip_prefix("./")
                .unwrap_or(path)
                .to_string_lossy()
        } else {
            path.to_string_lossy()
        };

        let result_path = if url_str.starts_with("../") {
            if let Some(parent) = base_path.parent() {
                let mut path_buf = PathBuf::from(parent);
                path_buf.push(&*clean_url);
                path_buf
            } else {
                PathBuf::from(&*clean_url)
            }
        } else {
            let mut path_buf = PathBuf::from(base_path);
            path_buf.push(&*clean_url);
            path_buf
        };

        let result_str = result_path.to_string_lossy().to_string();
        if !Path::new(&result_str).is_absolute() {
            let mut new_path = PathBuf::from("/");
            new_path.push(result_str);
            return new_path.to_string_lossy().to_string();
        }
        return result_str;
    }

    url_str.to_string()
}

fn get_default_tags() -> FxHashMap<String, Vec<String>> {
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
    tags
}
