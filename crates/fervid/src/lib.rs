//! The main public crate of the `fervid` project.
//!
//! Here's how you can use `fervid` to generate a module from an SFC:
//! <p style="background:rgba(255,181,77,0.16);padding:0.75em;">
//! <strong>Warning:</strong> This example is very likely to change in the future.
//! Please note that fervid is still unstable.
//! </p>
//!
//! ```
//! let input = r#"
//!   <template><div>hello world</div></template>
//! "#;
//!
//! // Parse
//! let (remaining_input, sfc) = fervid::parser::core::parse_sfc(input).unwrap();
//!
//! // Find template block
//! let mut template_block = sfc.template;
//! let Some(ref mut template_block) = template_block else {
//!     panic!("No template block");
//! };
//!
//! // Do the necessary transformations
//! let mut scope_helper = fervid_transform::structs::ScopeHelper::default();
//! let module = fervid_transform::script::transform_and_record_scripts(sfc.script_setup, sfc.script_legacy, &mut scope_helper);
//! fervid_transform::template::transform_and_record_template(template_block, &mut scope_helper);
//!
//! // Create the context and generate the template block
//! let mut ctx = fervid_codegen::CodegenContext::default();
//! let template_expr = ctx.generate_sfc_template(&template_block);
//!
//! // Generate the module code
//! let sfc_module = ctx.generate_module(Some(template_expr), module.module, module.export_obj, module.setup_fn);
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
pub fn compile_sync_naive(source: &str) -> Result<String, String> {
    // let (_, mut sfc) = parse_sfc(&source).map_err(|err| {
    //     return err.to_string();
    // })?;

    let mut errors = Vec::new();
    let sfc = fervid_parser::parse_sfc(&source, &mut errors).map_err(|err| {
        return err.to_string();
    })?;

    // TODO Return template used variables as a part of transformation result.
    // Also `used_imports`? `vue_imports`? User imports?
    let transform_result = transform_sfc(sfc);

    let mut ctx = CodegenContext::default();
    ctx.used_imports = transform_result.used_vue_imports;

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
