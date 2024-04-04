use napi_derive::napi;
use swc_core::common::Spanned;

/// Fervid: a compiler for Vue.js written in Rust
#[napi(js_name = "Compiler")]
#[derive(Clone)]
pub struct FervidJsCompiler {
    pub options: FervidJsCompilerOptions,
}

/// Raw options passed from the Node.js side
#[napi(object)]
#[derive(Default, Clone)]
pub struct FervidJsCompilerOptions {
    /// Apply production optimizations. Default: false
    pub is_production: Option<bool>,

    /// TODO Support SSR
    /// Enable SSR. Default: false
    pub ssr: Option<bool>,

    /// TODO Find a performant solution to source-maps
    /// TODO Implement source-maps
    /// Enable source maps
    pub source_map: Option<bool>,

    /// Script compilation options
    pub script: Option<FervidJsCompilerOptionsScript>,

    /// Template compilation options
    pub template: Option<FervidJsCompilerOptionsTemplate>,

    /// Style compilation options
    pub style: Option<FervidJsCompilerOptionsStyle>,

    /// TODO Regex handling logic is needed (plus sanitation)
    /// TODO Implement custom element mode (low priority)
    /// Transform Vue SFCs into custom elements.
    ///  - `true`: all `*.vue` imports are converted into custom elements
    ///  - `string | RegExp`: matched files are converted into custom elements
    /// Default: files ending with `.ce.vue`
    pub custom_element: Option<()>,
    // Ignored
    // pub compiler: Option<()>,

    // Ignored, will be determined automatically based on `is_production` and `script` tags
    // pub inline_template: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct FervidJsCompilerOptionsTemplate {}

#[napi(object)]
#[derive(Clone)]
pub struct FervidJsCompilerOptionsScript {
    /// Ignored
    /// Hoist <script setup> static constants.
    /// - Only enabled when one `<script setup>` exists.
    /// Default: true
    pub hoist_static: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct FervidJsCompilerOptionsStyle {
    /// Ignored
    pub trim: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct FervidCompileOptions {
    /// Scope ID for prefixing injected CSS variables
    pub id: String,

    /// Filename is used for automatic component name inference and self-referential imports
    pub filename: String,
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
