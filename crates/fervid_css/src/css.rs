mod codegen;
mod error;
mod parse;
mod transform;

use fervid_core::error::Severity;
use swc_core::common::Span;
use swc_css_parser::parser::ParserConfig;

pub use codegen::{stringify, StringifyOptions};
pub use error::CssError;
pub use parse::parse_stylesheet;
pub use transform::ScopedTransformer;

#[derive(Default)]
pub struct TransformCssConfig {
    pub parse: ParserConfig,
    pub stringify: StringifyOptions,
}

/// Transforms raw CSS, also handles the scopes.
pub fn transform_css(
    content: &str,
    span: Span,
    scope: Option<&str>,
    errors: &mut Vec<CssError>,
    config: TransformCssConfig,
) -> Option<String> {
    // Parse and collect errors
    let mut parse_errors = Vec::new();
    let parse_result = parse_stylesheet(content, span, config.parse, &mut parse_errors);
    let is_recoverable = parse_result.is_ok();
    errors.extend(parse_errors.into_iter().map(|e| {
        if is_recoverable {
            CssError::ParseRecoverable(e)
        } else {
            CssError::ParseUnrecoverable(e)
        }
    }));

    let Ok(mut stylesheet) = parse_result else {
        return None;
    };

    // Transform and check for unrecoverable errors
    if let Some(scope) = scope {
        let mut transformer = ScopedTransformer::new(scope);
        transformer.transform(&mut stylesheet);
        errors.append(&mut transformer.take_errors());
    }
    if errors.iter().any(Severity::is_unrecoverable_error) {
        return None;
    }

    Some(stringify(&stylesheet, config.stringify))
}
