extern crate lazy_static;

mod analyzer;
mod compiler;
mod parser;

pub use analyzer::ast_optimizer::optimize_ast;
pub use compiler::codegen::compile_ast;
pub use parser::core::parse_sfc;
pub use parser::sfc_blocks::*;
pub use parser::structs::*;

#[allow(dead_code)]
pub fn test_swc_transform(source_code: &str) -> Option<String> {
    compiler::transform::swc::transform_scoped(
        source_code,
        &Default::default(),
        0
    )
}
