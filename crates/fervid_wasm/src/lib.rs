extern crate wee_alloc;

// Use `wee_alloc` as the global allocator.
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use wasm_bindgen::prelude::*;
use fervid::compile_sync_naive;

#[wasm_bindgen]
pub fn compile_sync(source: &str, is_prod: Option<bool>) -> Result<String, String> {
    compile_sync_naive(source, is_prod.unwrap_or(false))
}
