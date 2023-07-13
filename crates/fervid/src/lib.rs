extern crate lazy_static;

pub mod analyzer;
pub mod compiler;
pub mod parser;

pub use analyzer::ast_optimizer::optimize_template;
pub use compiler::codegen::compile_sfc;
pub use parser::core::parse_sfc;
pub use fervid_core::*;
use parser::ecma::parse_js;

#[allow(dead_code)]
pub fn test_swc_transform(source_code: &str) -> Option<String> {
    let Ok(mut expr) = parse_js(source_code, 0, 0) else {
        return None;
    };

    compiler::transform::swc::transform_scoped(
        &mut expr,
        &Default::default(),
        0
    )
}
