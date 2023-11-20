//! The main public crate of the `fervid` project.
//!
//! Here's how you can use `fervid` to generate a module from an SFC:
//! <p style="background:rgba(255,181,77,0.16);padding:0.75em;">
//! <strong>Warning:</strong> This example is very likely to change in the future.
//! Please note that fervid is still unstable.
//! </p>
//!
//! ```
//! use swc_core::ecma::ast::Expr;
//!
//! let input = r#"
//!   <template><div>hello world</div></template>
//! "#;
//!
//! // Parse
//! let (remaining_input, sfc) = fervid::parser::core::parse_sfc(input).unwrap();
//! 
//! // Do the necessary transformations
//! let is_prod = true;
//! let transform_result = fervid_transform::transform_sfc(sfc, is_prod);
//!
//! // Create the context and generate the template block
//! let mut ctx = fervid_codegen::CodegenContext::with_bindings_helper(transform_result.bindings_helper);
//!
//! let template_expr: Option<Expr> = transform_result.template_block.map(|template_block| {
//!     ctx.generate_sfc_template(&template_block)
//! });
//!
//! // Generate the module code
//! let sfc_module = ctx.generate_module(
//!     template_expr,
//!     transform_result.module,
//!     transform_result.exported_obj,
//!     transform_result.setup_fn,
//! );
//!
//! // (Optional) Stringify the code
//! let compiled_code = fervid_codegen::CodegenContext::stringify(input, &sfc_module, false);
//! ```

extern crate lazy_static;

pub mod parser;

use fervid_codegen::CodegenContext;
pub use fervid_core::*;
use fervid_transform::transform_sfc;
use swc_core::ecma::ast::Expr;

/// Naive implementation of the SFC compilation, meaning that:
/// - it handles the standard flow without plugins;
/// - it compiles to `String` instead of SWC module;
/// - it does not report errors.
/// This implementation is mostly meant for the WASM and NAPI beta.
/// Later on, it will be replaced with a stable API.
pub fn compile_sync_naive(source: &str, is_prod: bool) -> Result<String, String> {
    // let (_, mut sfc) = parse_sfc(&source).map_err(|err| {
    //     return err.to_string();
    // })?;

    let mut errors = Vec::new();
    let mut parser = fervid_parser::SfcParser::new(source, &mut errors);
    let sfc = parser.parse_sfc().map_err(|err| {
        return err.to_string();
    })?;

    // TODO Return template used variables as a part of transformation result.
    // Also `used_imports`? `vue_imports`? User imports?
    let transform_result = transform_sfc(sfc, is_prod);

    let mut ctx = CodegenContext::with_bindings_helper(transform_result.bindings_helper);

    let template_expr: Option<Expr> = transform_result.template_block.map(|template_block| {
        ctx.generate_sfc_template(&template_block)
    });

    let sfc_module = ctx.generate_module(
        template_expr,
        transform_result.module,
        transform_result.exported_obj,
        transform_result.setup_fn,
    );

    let compiled_code = CodegenContext::stringify(&source, &sfc_module, false);

    Ok(compiled_code)
}
