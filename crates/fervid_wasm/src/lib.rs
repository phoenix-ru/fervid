extern crate wee_alloc;

// Use `wee_alloc` as the global allocator.
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use fervid::{compile, CompileOptions, CompileResult, PropsDestructureConfig};
use swc_core::common::{sync::Lrc, SourceMap, Spanned};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmCompileError {
    pub start_line_number: usize,
    pub end_line_number: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub message: String,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmCompileResult {
    pub code: String,
    pub errors: Vec<WasmCompileError>,
}

#[wasm_bindgen]
pub fn compile_sync(source: &str, is_prod: Option<bool>) -> Result<WasmCompileResult, String> {
    // compile_sync_naive(source, is_prod.unwrap_or(false))
    let compile_result = compile(
        source,
        CompileOptions {
            filename: "anonymous.vue".into(),
            id: "".into(),
            is_prod,
            is_custom_element: Some(false),
            props_destructure: Some(PropsDestructureConfig::True),
            ssr: Some(false),
            gen_default_as: None,
            source_map: None,
            transform_asset_urls: None,
        },
    );

    match compile_result {
        Ok(compiled) => Ok(convert_compile_result(compiled, source)),

        Err(e) => Err(e.to_string()),
    }
}

fn convert_compile_result(compiled: CompileResult, source: &str) -> WasmCompileResult {
    let code = compiled.code;

    let mut errors = vec![];
    if !compiled.errors.is_empty() {
        let cm: Lrc<SourceMap> = Default::default();
        cm.new_source_file(
            Lrc::new(swc_core::common::FileName::Anon),
            source.to_owned(),
        );
        errors.reserve(compiled.errors.len());

        for error in compiled.errors {
            let span = error.span();
            let start = cm.lookup_char_pos(span.lo);
            let end = cm.lookup_char_pos(span.hi);
            errors.push(WasmCompileError {
                start_line_number: start.line,
                end_line_number: end.line,
                start_column: start.col.0,
                end_column: end.col.0,
                message: error.to_string(),
            })
        }
    }

    WasmCompileResult { code, errors }
}
