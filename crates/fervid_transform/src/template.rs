//! Handles template AST transformations.

mod ast_transform;
mod collect_vars;
mod expr_transform;
mod js_builtins;
mod resolutions;
mod v_on;
mod utils;

pub use ast_transform::transform_and_record_template;
