use std::{fmt::Write, path::PathBuf};

use fervid_core::{
    fervid_atom, AttributeOrBinding, ElementNode, FervidAtom, StrOrExpr, VBindDirective,
};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::ast::{
        BinExpr, BinaryOp, Expr, Ident, ImportDecl, ImportDefaultSpecifier, ImportSpecifier, Lit,
        Str,
    },
};
use url::Url;

use crate::{
    error::{TemplateError, TemplateErrorKind, TransformError},
    TransformAssetUrlsConfig, TransformAssetUrlsConfigOptions, TransformSfcContext,
};

lazy_static! {
    pub static ref DEFAULT_OPTIONS: TransformAssetUrlsConfigOptions =
        TransformAssetUrlsConfigOptions::default();

    // Adapted from https://github.com/kardeiz/pathetic/blob/3991836305b264aa7579de6ea81aed930cff172b/src/lib.rs#L66-L71
    pub static ref BASE_URL: Url = "http://_".parse().expect("`http://_` is a valid `URL`");
}

pub fn transform_asset_urls(element_node: &mut ElementNode, ctx: &mut TransformSfcContext) {
    match &ctx.transform_asset_urls {
        TransformAssetUrlsConfig::Disabled => return,
        TransformAssetUrlsConfig::EnabledDefault => {
            transform_element_asset_urls(
                element_node,
                &DEFAULT_OPTIONS,
                &mut ctx.errors,
                &mut ctx.bindings_helper.imports,
            );
        }
        TransformAssetUrlsConfig::EnabledOptions(ref opts) => {
            transform_element_asset_urls(
                element_node,
                opts,
                &mut ctx.errors,
                &mut ctx.bindings_helper.imports,
            );
        }
    }
}

// Adapted from https://github.com/vuejs/core/blob/3f27c58ffbd4309df369bc89493fdc284dc540bb/packages/compiler-sfc/src/template/transformAssetUrl.ts#L72-L148
fn transform_element_asset_urls(
    element: &mut ElementNode,
    options: &TransformAssetUrlsConfigOptions,
    errors: &mut Vec<TransformError>,
    imports: &mut Vec<ImportDecl>,
) {
    let tags = &options.tags;

    let empty = vec![];
    let attrs = tags.get(&element.starting_tag.tag_name).unwrap_or(&empty);
    let wild_card_attrs = tags.get(&fervid_atom!("*")).unwrap_or(&empty);

    if attrs.is_empty() && wild_card_attrs.is_empty() {
        return;
    }

    for attr in element.starting_tag.attributes.iter_mut() {
        let AttributeOrBinding::RegularAttribute { name, value, span } = attr else {
            continue;
        };

        if (!attrs.contains(&name) && !wild_card_attrs.contains(&name))
            || value.trim().is_empty()
            || is_external_url(&value)
            || is_data_url(&value)
            || value.starts_with('#')
            || (!options.include_absolute && !is_relative_url(&value))
        {
            continue;
        }

        macro_rules! bail {
            ($err_kind: ident) => {{
                errors.push(TransformError::TemplateError(TemplateError {
                    kind: TemplateErrorKind::$err_kind,
                    span: *span,
                }));
                continue;
            }};
        }

        if let (Some(base_str), Some('.')) = (options.base.as_ref(), value.chars().nth(0)) {
            // explicit base - directly rewrite relative urls into absolute url
            // to avoid generating extra imports
            // Allow for full hostnames provided in options.base
            let Ok(base) = parse_url(&base_str) else {
                bail!(TransformAssetUrlsBaseUrlParseFailed)
            };

            // Match the behavior of the official compiler as close as possible
            let mut final_result = String::new();

            // Because `url::Url` parses with a dummy base of `http://_`, we need to check if the result is user-provided or a dummy
            let base_starts_with_double_slash = base_str.starts_with("//");
            let is_dummy = base.scheme() == "http"
                && !base_str.starts_with("http")
                && !base_starts_with_double_slash;

            // Add protocol and host
            if !is_dummy {
                final_result.reserve(base_str.len());

                let protocol = base.scheme();
                if let Some(host_str) = base.host_str() {
                    if base_starts_with_double_slash {
                        // Special handling of `//` user strings, e.g. `//example.com`
                        final_result.push_str("//");
                    } else {
                        final_result.push_str(protocol);
                        final_result.push_str("://");
                    }
                    final_result.push_str(host_str);
                    if let Some(port) = base.port() {
                        let _ = write!(final_result, ":{}", port);
                    }
                }
            }

            // Join the path using `PathBuf` instead of `Url` to mimic the official compiler
            let path_buf: PathBuf = [base.path(), strip_prefix(&value)].iter().collect();

            for path_cmp in path_buf.components() {
                match path_cmp {
                    std::path::Component::Prefix(prefix_component) => {
                        let Some(s) = prefix_component.as_os_str().to_str() else {
                            bail!(TransformAssetUrlsUrlParseFailed)
                        };
                        final_result.push_str(s);
                    }
                    std::path::Component::RootDir => {}
                    // `PathBuf::components` normalizes `.` away - if it was left, it is likely at the beginning
                    std::path::Component::CurDir => final_result.push('.'),
                    // Push the parent directory because
                    std::path::Component::ParentDir => final_result.push_str(".."),
                    std::path::Component::Normal(os_str) => {
                        let Some(s) = os_str.to_str() else {
                            bail!(TransformAssetUrlsUrlParseFailed)
                        };
                        final_result.push('/');
                        final_result.push_str(s);
                    }
                }
            }

            // let Some(path_buf_str) = path_buf.to_str() else {
            //     bail!(TransformAssetUrlsUrlParseFailed)
            // };

            // dbg!(path_buf_str);

            // final_result.push_str(path_buf_str);

            *value = FervidAtom::from(final_result);
            continue;
        }

        // There is no good solution for parsing `value` while preserving the original directory signifiers.
        // Parsing using `url::Url` will actively remove any prefix `.` symbols or similar.
        // Unfortunately, reproducing `Node.js`s non-standard `url.parse` in Rust is not possible/feasible,
        // thus we assume that passed string is a valid path already.
        let mut path = strip_prefix(&value);
        let mut hash = None;
        if let Some(hash_pos) = path.find('#') {
            hash = Some(&path[hash_pos..]);
            path = &path[..hash_pos];
        }

        let import_expr = match get_import_expression(path, hash, *span, imports) {
            Ok(v) => v,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };

        *attr = AttributeOrBinding::VBind(VBindDirective {
            argument: Some(StrOrExpr::Str(name.to_owned())),
            value: import_expr,
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: *span,
        });
    }
}

fn get_import_expression(
    path: &str,
    hash: Option<&str>,
    span: Span,
    imports: &mut Vec<ImportDecl>,
) -> Result<Box<Expr>, TransformError> {
    if path.is_empty() {
        return Ok(Box::new(Expr::Lit(Lit::Str(Str {
            span,
            value: fervid_atom!(""),
            raw: None,
        }))));
    }

    let existing_index = imports.iter().position(|it| it.src.value == path);

    let name_ident = if let Some(existing_index) = existing_index {
        Ident {
            sym: FervidAtom::from(format!("_imports_{existing_index}")),
            ..Default::default()
        }
    } else {
        let name_ident = Ident {
            sym: FervidAtom::from(format!("_imports_{}", imports.len())),
            ..Default::default()
        };

        let specifier = ImportSpecifier::Default(ImportDefaultSpecifier {
            span: DUMMY_SP,
            local: name_ident.to_owned(),
        });

        let decoded_path = percent_encoding::percent_decode_str(path);
        let Ok(decoded) = decoded_path.decode_utf8() else {
            return Err(TransformError::TemplateError(TemplateError {
                span,
                kind: TemplateErrorKind::TransformAssetUrlsUrlParseFailed,
            }));
        };

        imports.push(ImportDecl {
            span: DUMMY_SP,
            specifiers: vec![specifier],
            src: Box::new(Str {
                span,
                value: FervidAtom::from(decoded),
                raw: None,
            }),
            type_only: false,
            with: None,
            phase: Default::default(),
        });

        name_ident
    };

    let name_expr = Box::new(Expr::Ident(name_ident));

    let Some(hash) = hash else {
        return Ok(name_expr);
    };

    let hash_exp = Box::new(Expr::Bin(BinExpr {
        span,
        op: BinaryOp::Add,
        left: name_expr,
        right: Box::new(Expr::Lit(Lit::Str(Str {
            span,
            value: FervidAtom::from(hash),
            raw: None,
        }))),
    }));

    // TODO Hoisting
    Ok(hash_exp)
}

fn parse_url(url: &str) -> Result<Url, url::ParseError> {
    BASE_URL.clone().join(strip_prefix(url))
}

fn strip_prefix(url: &str) -> &str {
    let mut url = url;
    if let Some(stripped_one) = url.strip_prefix('~') {
        if let Some(stripped_two) = stripped_one.strip_prefix('/') {
            url = stripped_two;
        } else {
            url = stripped_one;
        }
    }
    url
}

fn is_relative_url(url: &FervidAtom) -> bool {
    let first_char = url.chars().nth(0);
    matches!(first_char, Some('.' | '~' | '@'))
}

/// https://github.com/vuejs/core/blob/3f27c58ffbd4309df369bc89493fdc284dc540bb/packages/compiler-sfc/src/template/templateUtils.ts#L9-L12
fn is_external_url(url: &FervidAtom) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//")
}

/// https://github.com/vuejs/core/blob/3f27c58ffbd4309df369bc89493fdc284dc540bb/packages/compiler-sfc/src/template/templateUtils.ts#L14-L17
fn is_data_url(url: &FervidAtom) -> bool {
    url.trim_start().starts_with("data:")
}
