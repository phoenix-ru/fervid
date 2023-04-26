extern crate lazy_static;

pub mod analyzer;
pub mod compiler;
pub mod parser;

pub use analyzer::ast_optimizer::optimize_template;
pub use compiler::codegen::compile_sfc;
pub use parser::core::parse_sfc;
pub use fervid_core::*;

#[allow(dead_code)]
pub fn test_swc_transform(source_code: &str) -> Option<String> {
    compiler::transform::swc::transform_scoped(
        source_code,
        &Default::default(),
        0
    )
}
