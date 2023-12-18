#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::compile;
use swc_core::common::Spanned;

#[napi(object)]
pub struct CompileSyncOptions {
    pub is_prod: bool,
}

#[napi(object)]
pub struct CompileResult {
    pub code: String,
    pub styles: Vec<Style>,
    pub errors: Vec<SerializedError>,
    pub custom_blocks: Vec<CustomBlock>,
}

#[napi(object)]
pub struct Style {
    pub code: String,
    pub is_compiled: bool,
    pub lang: String,
    pub is_scoped: bool,
}

#[napi(object)]
pub struct CustomBlock {
    pub content: String,
    pub lo: u32,
    pub hi: u32,
    pub tag_name: String,
}

#[napi(object)]
pub struct SerializedError {
    pub lo: u32,
    pub hi: u32,
    pub message: String,
}

#[napi]
pub fn compile_sync(source: String, options: Option<CompileSyncOptions>) -> Result<CompileResult> {
    compile_and_convert(&source, options.as_ref())
}

#[napi]
pub fn compile_async(
    source: String,
    options: Option<CompileSyncOptions>,
    signal: Option<AbortSignal>,
) -> AsyncTask<CompileTask> {
    let task = CompileTask {
        input: source,
        options,
    };
    AsyncTask::with_optional_signal(task, signal)
}

pub struct CompileTask {
    input: String,
    options: Option<CompileSyncOptions>,
}

#[napi]
impl Task for CompileTask {
    type JsValue = CompileResult;
    type Output = CompileResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        compile_and_convert(&self.input, self.options.as_ref())
    }

    fn resolve(&mut self, _env: Env, result: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(result)
    }
}

fn compile_and_convert(
    source: &str,
    options: Option<&CompileSyncOptions>,
) -> Result<CompileResult> {
    let is_prod = options.map_or(false, |v| v.is_prod);

    let native_compile_result =
        compile(source, is_prod).map_err(|e| Error::from_reason(e.to_string()))?;

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

impl From<fervid::CompileEmittedStyle> for Style {
    fn from(value: fervid::CompileEmittedStyle) -> Self {
        Self {
            code: value.code,
            is_compiled: value.is_compiled,
            lang: value.lang,
            is_scoped: value.is_scoped,
        }
    }
}

impl From<fervid::errors::CompileError> for SerializedError {
    fn from(value: fervid::errors::CompileError) -> Self {
        let span = value.span();
        SerializedError {
            lo: span.lo.0,
            hi: span.hi.0,
            message: value.to_string(),
        }
    }
}

impl From<fervid::CompileEmittedAsset> for CustomBlock {
    fn from(value: fervid::CompileEmittedAsset) -> Self {
        CustomBlock {
            content: value.content,
            lo: value.lo,
            hi: value.hi,
            tag_name: value.tag_name,
        }
    }
}
