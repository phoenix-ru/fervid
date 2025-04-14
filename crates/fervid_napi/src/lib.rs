#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use std::borrow::Cow;

use fervid_transform::PropsDestructureConfig;
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
        let compiled = compile_impl(self, &source, &options)?;
        Ok(convert(env, compiled, &options))
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
}

fn compile_impl(
    compiler: &FervidJsCompiler,
    source: &str,
    options: &FervidCompileOptions,
) -> Result<fervid::CompileResult> {
    let props_destructure = match options.props_destructure {
        Some(Either::A(true)) => Some(PropsDestructureConfig::True),
        Some(Either::A(false)) => Some(PropsDestructureConfig::False),
        Some(Either::B(ref s)) if s == "error" => Some(PropsDestructureConfig::Error),
        _ => None,
    };

    // Normalize options to the ones defined in fervid
    let compile_options = CompileOptions {
        filename: Cow::Borrowed(&options.filename),
        id: Cow::Borrowed(&options.id),
        is_prod: compiler.options.is_production,
        is_custom_element: options.is_custom_element,
        props_destructure,
        ssr: compiler.options.ssr,
        gen_default_as: options
            .gen_default_as
            .as_ref()
            .map(|v| Cow::Borrowed(v.as_str())),
        source_map: compiler.options.source_map,
    };

    compile(source, compile_options).map_err(|e| Error::from_reason(e.to_string()))
}

fn convert(
    env: Env,
    mut result: fervid::CompileResult,
    options: &FervidCompileOptions,
) -> CompileResult {
    // Serialize bindings if requested
    let setup_bindings = if matches!(options.output_setup_bindings, Some(true)) {
        env.create_object()
            .map(|mut obj| {
                for binding in result.setup_bindings.drain(..) {
                    let _ = obj.set(binding.sym.as_str(), BindingTypes::from(binding.binding_type));
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
        compile_impl(&self.compiler, &self.input, &self.options)
    }

    fn resolve(&mut self, env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(convert(env, result, &self.options))
    }
}
