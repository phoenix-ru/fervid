#![deny(clippy::all)]

// #[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
// #[global_allocator]
// static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use std::{borrow::Cow, marker::PhantomData, rc::Rc, sync::Arc};

use fervid_transform::{PropsDestructureConfig, TransformAssetUrlsConfig};
use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::{compile, errors::CompileError, CompileOptions};
use structs::{
    BindingTypes, CompileResult, FervidCompileOptions, FervidJsCompiler, FervidJsCompilerOptions,
};
use swc_core::common::{sync::Lrc, BytePos, SourceMap};

use crate::structs::SerializedError;

mod structs;

#[napi]
impl FervidJsCompiler {
    #[napi(constructor)]
    pub fn new(options: Option<FervidJsCompilerOptions>) -> Self {
        let options = options.unwrap_or_default();
        FervidJsCompiler {
            options: Arc::new(options),
        }
    }

    #[napi]
    pub fn compile_sync(
        &self,
        env: Env,
        source: String,
        options: FervidCompileOptions,
    ) -> Result<CompileResult> {
        let compiled = compile_impl(self, &source, &options)?;
        Ok(convert(env, compiled, &options, &self.options, &source))
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
            env: PhantomData,
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

    let transform_asset_urls =
        compiler
            .options
            .template
            .as_ref()
            .and_then(|v| match v.transform_asset_urls.as_ref() {
                Some(Either::A(true)) => Some(TransformAssetUrlsConfig::EnabledDefault),
                Some(Either::A(false)) => Some(TransformAssetUrlsConfig::Disabled),
                Some(Either::B(options)) => Some(TransformAssetUrlsConfig::EnabledOptions(
                    Rc::new(options.to_owned().into()),
                )),
                None => None,
            });

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
        transform_asset_urls,
    };

    compile(source, compile_options).map_err(|e| Error::from_reason(e.to_string()))
}

fn convert<'env>(
    env: Env,
    mut result: fervid::CompileResult,
    options: &FervidCompileOptions,
    compiler_options: &FervidJsCompilerOptions,
    source: &str,
) -> CompileResult<'env> {
    // Serialize bindings if requested
    let setup_bindings = if matches!(options.output_setup_bindings, Some(true)) {
        Object::new(&env)
            .map(|mut obj| {
                for binding in result.setup_bindings.drain(..) {
                    let _ = obj.set(
                        binding.sym.as_str(),
                        BindingTypes::from(binding.binding_type),
                    );
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
        errors: convert_errors(result.errors, compiler_options, source),
        styles: result
            .styles
            .into_iter()
            .map(|style| style.into())
            .collect(),
        setup_bindings,
    }
}

fn convert_errors(
    compile_errors: Vec<CompileError>,
    compiler_options: &FervidJsCompilerOptions,
    source: &str,
) -> Vec<SerializedError> {
    let mut errors: Vec<SerializedError> = compile_errors.into_iter().map(Into::into).collect();

    let Some(ref diagnostics) = compiler_options.diagnostics else {
        return errors;
    };

    if let Some(true) = diagnostics.error_lines_columns {
        let cm: Lrc<SourceMap> = Default::default();
        cm.new_source_file(
            Lrc::new(swc_core::common::FileName::Anon),
            source.to_owned(),
        );

        for error in errors.iter_mut() {
            let start = cm.lookup_char_pos(BytePos(error.lo));
            let end = cm.lookup_char_pos(BytePos(error.hi));
            error.start_line_number = start.line as u32;
            error.end_line_number = end.line as u32;
            error.start_column = start.col.0 as u32;
            error.end_column = end.col.0 as u32;
        }
    }

    errors
}

pub struct CompileTask<'env> {
    compiler: FervidJsCompiler,
    input: String,
    options: FervidCompileOptions,
    env: PhantomData<&'env ()>,
}

#[napi]
impl<'env> Task for CompileTask<'env> {
    type JsValue = CompileResult<'env>;
    type Output = fervid::CompileResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        compile_impl(&self.compiler, &self.input, &self.options)
    }

    fn resolve(&mut self, env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(convert(
            env,
            result,
            &self.options,
            &self.compiler.options,
            &self.input,
        ))
    }
}
