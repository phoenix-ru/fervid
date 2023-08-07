//! This crate is used for generating the final Module code of the SFC.
//!
//! The main structure of this crate is [CodegenContext].
//! Here's how you can use it to generate the module from an SFC:
//! <p style="background:rgba(255,181,77,0.16);padding:0.75em;">
//! <strong>Warning:</strong> This example is very likely to change in the future.
//! Please note that fervid is still unstable.
//! </p>
//! 
//! ```
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
//! let module = transform_and_record_scripts(sfc.script_setup, sfc.script_legacy, &mut scope_helper);
//! transform_and_record_template(template_block, &mut scope_helper);
//!
//! // Create the context and generate the template block
//! let mut ctx = fervid_codegen::CodegenContext::default();
//! let template_expr = ctx.generate_sfc_template(&template_block);
//!
//! // Generate the module code
//! let sfc_module = ctx.generate_module(template_expr, module.0, module.1);
//!
//! // (Optional) Stringify the code
//! let compiled_code = fervid_codegen::CodegenContext::stringify(test, &sfc_module, false);
//! ```

#[macro_use]
extern crate lazy_static;

mod atoms;
mod attributes;
mod comments;
mod components;
mod context;
mod control_flow;
mod directives;
mod interpolation;
mod elements;
mod imports;
mod text;
mod utils;

#[cfg(test)]
mod test_utils;

pub use context::CodegenContext;
