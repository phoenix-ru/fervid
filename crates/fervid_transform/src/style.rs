use fervid_core::{fervid_atom, SfcStyleBlock};
use fervid_css::*;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, IdentName, KeyValueProp, Lit, Prop, PropName, PropOrSpread, Str},
};

use crate::{error::TransformError, structs::TransformScriptsResult};

const CSS_PREFIX: &str = "data-v-";

/// Adds `__scopeId: scope`, e.g. `__scopeId: "data-v-7ba5bd90"`
pub fn attach_scope_id(transform_result: &mut TransformScriptsResult, scope: &str) {
    transform_result
        .export_obj
        .props
        .push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(IdentName {
                span: DUMMY_SP,
                sym: fervid_atom!("__scopeId"),
            }),
            value: Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: scope.into(),
                raw: None,
            }))),
        }))));
}

/// Constructs a style scope for a given file hash
pub fn create_style_scope(file_hash: &str) -> String {
    let mut scope = String::with_capacity(CSS_PREFIX.len() + file_hash.len());
    scope.push_str(CSS_PREFIX);
    scope.push_str(file_hash);
    scope
}

pub fn transform_style_blocks(
    style_blocks: &mut [SfcStyleBlock],
    scope: &str,
    errors: &mut Vec<TransformError>,
) -> bool {
    // Check work
    if !style_blocks.iter().any(should_transform_style_block) {
        return false;
    }

    // TODO Config
    // TODO Allow minifying CSS

    // Map errors from `fervid_css` to `fervid_transform`
    let mut css_errors = Vec::new();

    for style_block in style_blocks.iter_mut() {
        if style_block.is_scoped && style_block.lang == "css" {
            let result = transform_css(
                &style_block.content,
                style_block.span,
                Some(scope),
                &mut css_errors,
                TransformCssConfig::default(),
            );

            if let Some(transformed) = result {
                style_block.content = transformed.into();
            }
        }
    }

    errors.extend(css_errors.into_iter().map(From::from));

    true
}

#[inline]
pub fn should_transform_style_block(block: &SfcStyleBlock) -> bool {
    block.is_scoped && block.lang == "css"
}
