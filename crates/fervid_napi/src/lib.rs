#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::compile_sync_naive;

#[napi(object)]
pub struct CompileSyncOptions {
    pub is_prod: bool,
}

#[napi]
pub fn compile_sync(source: String, options: Option<CompileSyncOptions>) -> Result<String> {
    compile_sync_naive(&source, options.map_or(false, |v| v.is_prod))
        .map_err(|e| Error::from_reason(e))
}

#[napi]
pub fn compile_async(
    source: String,
    options: Option<CompileSyncOptions>,
    signal: Option<AbortSignal>,
) -> napi::Result<AsyncTask<CompileTask>> {
    let task = CompileTask {
        input: source,
        options: options.unwrap_or_else(|| CompileSyncOptions { is_prod: false }),
    };
    Ok(AsyncTask::with_optional_signal(task, signal))
}

pub struct CompileTask {
    input: String,
    options: CompileSyncOptions,
}

#[napi]
impl Task for CompileTask {
    type JsValue = String;
    type Output = String;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        compile_sync_naive(&self.input, self.options.is_prod).map_err(|e| Error::from_reason(e))
    }

    fn resolve(&mut self, _env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(result)
    }
}
