//! The main public crate of the `fervid` project.
//!
//! Here's how you can `fervid` to generate a module from an SFC:
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
//! let sfc_module = ctx.generate_module(template_expr, module.0, module.1);
//!
//! // (Optional) Stringify the code
//! let compiled_code = fervid_codegen::CodegenContext::stringify(input, &sfc_module, false);
//! ```

extern crate lazy_static;

pub mod parser;

pub use parser::core::parse_sfc;
pub use fervid_core::*;
