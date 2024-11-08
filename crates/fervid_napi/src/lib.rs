#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use std::borrow::Cow;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::{compile, CompileOptions};
use structs::{
    BindingTypes, CompileResult, FervidCompileOptions, FervidJsCompiler, FervidJsCompilerOptions,
};

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
        env: Env,
        source: String,
        options: FervidCompileOptions,
    ) -> Result<CompileResult> {
        let compiled = self.compile_impl(&source, &options)?;
        Ok(self.convert(env, compiled, &options))
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

    fn compile_impl(
        &self,
        source: &str,
        options: &FervidCompileOptions,
    ) -> Result<fervid::CompileResult> {
        // Normalize options to the ones defined in fervid
        let compile_options = CompileOptions {
            filename: Cow::Borrowed(&options.filename),
            id: Cow::Borrowed(&options.id),
            is_prod: self.options.is_production,
            ssr: self.options.ssr,
            gen_default_as: options
                .gen_default_as
                .as_ref()
                .map(|v| Cow::Borrowed(v.as_str())),
            source_map: self.options.source_map,
        };

        compile(source, compile_options).map_err(|e| Error::from_reason(e.to_string()))
    }

    fn convert(
        &self,
        env: Env,
        mut result: fervid::CompileResult,
        options: &FervidCompileOptions,
    ) -> CompileResult {
        // Serialize bindings if requested
        let setup_bindings = if matches!(options.output_setup_bindings, Some(true)) {
            env.create_object()
                .map(|mut obj| {
                    for binding in result.setup_bindings.drain(..) {
                        let _ = obj.set(binding.0.as_str(), BindingTypes::from(binding.1));
                    }
                    obj
                })
                .ok()
        } else {
            None
        };

        CompileResult {
            code: result.code,
            source_map: result.source_map,
            custom_blocks: result
                .other_assets
                .into_iter()
                .map(|asset| asset.into())
                .collect(),
            errors: result.errors.into_iter().map(|e| e.into()).collect(),
            styles: result
                .styles
                .into_iter()
                .map(|style| style.into())
                .collect(),
            setup_bindings,
        }
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
    type Output = fervid::CompileResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.compiler.compile_impl(&self.input, &self.options)
    }

    fn resolve(&mut self, env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(self.compiler.convert(env, result, &self.options))
    }
}
