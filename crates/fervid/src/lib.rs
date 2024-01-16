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
//! let mut transform_errors = Vec::new();
//! let transform_result = fervid_transform::transform_sfc(sfc, is_prod, "filehash", &mut transform_errors);
//!
//! // Create the context and generate the template block
//! let mut ctx = fervid_codegen::CodegenContext::with_bindings_helper(transform_result.bindings_helper);
//!
//! let template_expr: Option<Expr> = transform_result.template_block.and_then(|template_block| {
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

pub mod errors;
pub mod parser;

use errors::CompileError;
use fervid_codegen::CodegenContext;
pub use fervid_core::*;
use fervid_parser::SfcParser;
use fervid_transform::{style::should_transform_style_block, transform_sfc};
use std::hash::{DefaultHasher, Hash, Hasher};
use swc_core::ecma::ast::Expr;

// TODO Add severity to errors
// TODO Better structs

pub struct CompileResult {
    pub code: String,
    pub file_hash: String,
    pub errors: Vec<CompileError>,
    pub styles: Vec<CompileEmittedStyle>,
    pub other_assets: Vec<CompileEmittedAsset>,
}

pub struct CompileEmittedStyle {
    pub code: String,
    pub is_compiled: bool,
    pub lang: String,
    pub is_scoped: bool,
}

pub struct CompileEmittedAsset {
    pub lo: u32,
    pub hi: u32,
    pub tag_name: String,
    pub content: String,
}

/// A more general-purpose SFC compilation function.
/// Not production-ready yet.
pub fn compile(source: &str, is_prod: bool) -> Result<CompileResult, CompileError> {
    let mut all_errors = Vec::<CompileError>::new();

    // Parse
    let mut sfc_parsing_errors = Vec::new();
    let mut parser = SfcParser::new(source, &mut sfc_parsing_errors);
    let sfc = parser.parse_sfc()?;
    all_errors.extend(sfc_parsing_errors.into_iter().map(From::from));

    // For scopes
    let file_hash = {
        let mut hasher = DefaultHasher::default();
        source.hash(&mut hasher);
        let num = hasher.finish();
        format!("{:x}", num)
    };

    // Transform
    let mut transform_errors = Vec::new();
    let transform_result = transform_sfc(sfc, is_prod, &file_hash, &mut transform_errors);
    all_errors.extend(transform_errors.into_iter().map(From::from));

    // Codegen
    let mut ctx = CodegenContext::with_bindings_helper(transform_result.bindings_helper);

    let template_expr: Option<Expr> = transform_result
        .template_block
        .and_then(|template_block| ctx.generate_sfc_template(&template_block));

    let sfc_module = ctx.generate_module(
        template_expr,
        transform_result.module,
        transform_result.exported_obj,
        transform_result.setup_fn,
    );

    let code = CodegenContext::stringify(&source, &sfc_module, false);

    let styles = transform_result
        .style_blocks
        .into_iter()
        .map(|style_block| CompileEmittedStyle {
            code: style_block.content.to_string(),
            is_compiled: should_transform_style_block(&style_block),
            lang: style_block.lang.to_string(),
            is_scoped: style_block.is_scoped,
        })
        .collect();

    let other_assets = transform_result
        .custom_blocks
        .into_iter()
        .map(|block| {
            CompileEmittedAsset {
                lo: 0, // todo
                hi: 0, // todo
                tag_name: block.starting_tag.tag_name.to_string(),
                content: block.content.to_string(),
            }
        })
        .collect();

    Ok(CompileResult {
        code,
        file_hash,
        errors: all_errors,
        styles,
        other_assets,
    })
}

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

    // Parse
    let mut errors = Vec::new();
    let mut parser = SfcParser::new(source, &mut errors);
    let sfc = parser.parse_sfc().map_err(|err| {
        return err.to_string();
    })?;

    // For scopes
    let file_hash = {
        let mut hasher = DefaultHasher::default();
        source.hash(&mut hasher);
        let num = hasher.finish();
        format!("{:x}", num)
    };

    // Transform
    let mut transform_errors = Vec::new();
    let transform_result = transform_sfc(sfc, is_prod, &file_hash, &mut transform_errors);

    // Codegen
    let mut ctx = CodegenContext::with_bindings_helper(transform_result.bindings_helper);

    let template_expr: Option<Expr> = transform_result
        .template_block
        .and_then(|template_block| ctx.generate_sfc_template(&template_block));

    let sfc_module = ctx.generate_module(
        template_expr,
        transform_result.module,
        transform_result.exported_obj,
        transform_result.setup_fn,
    );

    let compiled_code = CodegenContext::stringify(&source, &sfc_module, false);

    Ok(compiled_code)
}
