#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use std::borrow::Cow;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::{compile, CompileOptions};
use structs::{CompileResult, FervidCompileOptions, FervidJsCompiler, FervidJsCompilerOptions};

mod structs;

#[napi]
impl FervidJsCompiler {
    #[napi(constructor)]
    pub fn new(options: Option<FervidJsCompilerOptions>) -> Self {
        let options = options.unwrap_or_else(Default::default);
        FervidJsCompiler { options }
    }

    #[napi]
    pub fn compile_sync(
        &self,
        source: String,
        options: FervidCompileOptions,
    ) -> Result<CompileResult> {
        self.compile_and_convert(&source, &options)
    }

    #[napi]
    pub fn compile_async(
        &self,
        source: String,
        options: FervidCompileOptions,
        signal: Option<AbortSignal>,
    ) -> AsyncTask<CompileTask> {
        let task = CompileTask {
            compiler: self.to_owned(),
            input: source,
            options,
        };
        AsyncTask::with_optional_signal(task, signal)
    }

    fn compile_and_convert(
        &self,
        source: &str,
        options: &FervidCompileOptions,
    ) -> Result<CompileResult> {
        // Normalize options to the ones defined in fervid
        let compile_options = CompileOptions {
            filename: Cow::Borrowed(&options.filename),
            id: Cow::Borrowed(&options.id),
            is_prod: self.options.is_production,
            ssr: self.options.ssr,
            gen_default_as: options.gen_default_as.as_ref().map(|v| Cow::Borrowed(v.as_str())),
            source_map: self.options.source_map
        };

        let native_compile_result =
            compile(source, compile_options).map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(CompileResult {
            code: native_compile_result.code,
            source_map: native_compile_result.source_map,
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
    options: FervidCompileOptions,
}

#[napi]
impl Task for CompileTask {
    type JsValue = CompileResult;
    type Output = CompileResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.compiler.compile_and_convert(&self.input, &self.options)
    }

    fn resolve(&mut self, _env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(result)
    }
}
