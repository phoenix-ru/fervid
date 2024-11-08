use napi::JsObject;
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
    /// Produce source maps
    pub source_map: Option<bool>,
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
    /// Generate a const instead of default export
    pub gen_default_as: Option<String>,
    /// Whether setup bindings need to be serialized
    pub output_setup_bindings: Option<bool>,
}

#[napi(object)]
pub struct CompileResult {
    pub code: String,
    pub styles: Vec<Style>,
    pub errors: Vec<SerializedError>,
    pub custom_blocks: Vec<CustomBlock>,
    pub source_map: Option<String>,
    #[napi(ts_type = "Record<string, BindingTypes> | undefined")]
    pub setup_bindings: Option<JsObject>,
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

/// This is a copied enum from `fervid_core` with `napi` implementation to avoid littering the core crate.
///
/// The type of a binding (or identifier) which is used to show where this binding came from,
/// e.g. `Data` is for Options API `data()`, `SetupRef` if for `ref`s and `computed`s in Composition API.
///
/// <https://github.com/vuejs/core/blob/020851e57d9a9f727c6ea07e9c1575430af02b73/packages/compiler-core/src/options.ts#L76>
#[napi]
pub enum BindingTypes {
    /// returned from data()
    Data,
    /// declared as a prop
    Props,
    /// a local alias of a `<script setup>` destructured prop.
    /// the original is stored in __propsAliases of the bindingMetadata object.
    PropsAliased,
    /// a let binding (may or may not be a ref)
    SetupLet,
    /// a const binding that can never be a ref.
    /// these bindings don't need `unref()` calls when processed in inlined
    /// template expressions.
    SetupConst,
    /// a const binding that does not need `unref()`, but may be mutated.
    SetupReactiveConst,
    /// a const binding that may be a ref
    SetupMaybeRef,
    /// bindings that are guaranteed to be refs
    SetupRef,
    /// declared by other options, e.g. computed, inject
    Options,
    /// a literal constant, e.g. 'foo', 1, true
    LiteralConst,

    // Introduced by fervid:
    /// a `.vue` import or `defineComponent` call
    Component,
    /// an import which is not a `.vue` or `from 'vue'`
    Imported,
    /// a variable from the template
    TemplateLocal,
    /// a variable in the global Javascript context, e.g. `Array` or `undefined`
    JsGlobal,
    /// a non-resolved variable, presumably from the global Vue context
    Unresolved,
}

impl From<fervid::BindingTypes> for BindingTypes {
    fn from(value: fervid::BindingTypes) -> Self {
        match value {
            fervid::BindingTypes::Data => BindingTypes::Data,
            fervid::BindingTypes::Props => BindingTypes::Props,
            fervid::BindingTypes::PropsAliased => BindingTypes::PropsAliased,
            fervid::BindingTypes::SetupLet => BindingTypes::SetupLet,
            fervid::BindingTypes::SetupConst => BindingTypes::SetupConst,
            fervid::BindingTypes::SetupReactiveConst => BindingTypes::SetupReactiveConst,
            fervid::BindingTypes::SetupMaybeRef => BindingTypes::SetupMaybeRef,
            fervid::BindingTypes::SetupRef => BindingTypes::SetupRef,
            fervid::BindingTypes::Options => BindingTypes::Options,
            fervid::BindingTypes::LiteralConst => BindingTypes::LiteralConst,
            fervid::BindingTypes::Component => BindingTypes::Component,
            fervid::BindingTypes::Imported => BindingTypes::Imported,
            fervid::BindingTypes::TemplateLocal => BindingTypes::TemplateLocal,
            fervid::BindingTypes::JsGlobal => BindingTypes::JsGlobal,
            fervid::BindingTypes::Unresolved => BindingTypes::Unresolved,
        }
    }
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
