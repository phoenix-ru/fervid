#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::compile_sync_naive;

#[napi]
pub fn compile_sync(source: String) -> Result<String> {
    compile_sync_naive(&source).map_err(|e| Error::from_reason(e))
}
