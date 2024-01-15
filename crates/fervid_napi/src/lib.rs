#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::compile;
use structs::{CompileResult, FervidJsCompiler, FervidJsCompilerOptions};

mod structs;

#[napi]
impl FervidJsCompiler {
    #[napi(constructor)]
    pub fn new(options: Option<FervidJsCompilerOptions>) -> Self {
        let options = options.unwrap_or_else(Default::default);

        FervidJsCompiler {
            is_production: options.is_production.unwrap_or(false),
            ssr: options.ssr.unwrap_or(false),
            source_map: options.source_map.unwrap_or(false),
        }
    }

    #[napi]
    pub fn compile_sync(&self, source: String) -> Result<CompileResult> {
        self.compile_and_convert(&source)
    }

    #[napi]
    pub fn compile_async(
        &self,
        source: String,
        signal: Option<AbortSignal>,
    ) -> AsyncTask<CompileTask> {
        let task = CompileTask {
            compiler: self.to_owned(),
            input: source,
        };
        AsyncTask::with_optional_signal(task, signal)
    }

    fn compile_and_convert(&self, source: &str) -> Result<CompileResult> {
        let native_compile_result =
            compile(source, self.is_production).map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(CompileResult {
            code: native_compile_result.code,
            custom_blocks: native_compile_result
                .other_assets
                .into_iter()
                .map(|asset| asset.into())
                .collect(),
            errors: native_compile_result
                .errors
                .into_iter()
                .map(|e| e.into())
                .collect(),
            styles: native_compile_result
                .styles
                .into_iter()
                .map(|style| style.into())
                .collect(),
        })
    }
}

pub struct CompileTask {
    compiler: FervidJsCompiler,
    input: String,
}

#[napi]
impl Task for CompileTask {
    type JsValue = CompileResult;
    type Output = CompileResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.compiler.compile_and_convert(&self.input)
    }

    fn resolve(&mut self, _env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(result)
    }
}
