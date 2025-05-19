use std::collections::HashMap;

use fervid::FervidAtom;
use fervid_transform::TransformAssetUrlsConfigOptions;
use fxhash::FxHashMap;
use napi::{Either, JsObject};
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
pub struct FervidJsCompilerOptionsTemplate {
    /// Options for transforming asset URLs in template
    #[napi(js_name = "transformAssetUrls")]
    pub transform_asset_urls: Option<Either<bool, FervidTransformAssetUrlsOptions>>,
}

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
#[derive(Clone, Debug)]
pub struct FervidCompileOptions {
    /// Scope ID for prefixing injected CSS variables
    pub id: String,

    /// Filename is used for automatic component name inference and self-referential imports
    pub filename: String,

    /// Is the currently compiled file a custom element.
    /// To give more flexibility, this option only accepts a boolean, allowing to compute the value on the JS side,
    /// instead of relying on a hacky RegEx/JS function calls from the Fervid side.
    pub is_custom_element: Option<bool>,

    /// Generate a const instead of default export
    pub gen_default_as: Option<String>,

    /// Enable, disable or error on props destructure
    #[napi(ts_type = "boolean | 'error'")]
    pub props_destructure: Option<Either<bool, String>>,

    /// Whether setup bindings need to be serialized
    pub output_setup_bindings: Option<bool>,
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct FervidTransformAssetUrlsOptions {
    pub base: Option<String>,
    pub include_absolute: Option<bool>,
    pub tags: Option<HashMap<String, Vec<String>>>,
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
#[allow(non_camel_case_types)]
pub enum BindingTypes {
    /// returned from data()
    DATA,
    /// declared as a prop
    PROPS,
    /// a local alias of a `<script setup>` destructured prop.
    /// the original is stored in __propsAliases of the bindingMetadata object.
    PROPS_ALIASED,
    /// a let binding (may or may not be a ref)
    SETUP_LET,
    /// a const binding that can never be a ref.
    /// these bindings don't need `unref()` calls when processed in inlined
    /// template expressions.
    SETUP_CONST,
    /// a const binding that does not need `unref()`, but may be mutated.
    SETUP_REACTIVE_CONST,
    /// a const binding that may be a ref
    SETUP_MAYBE_REF,
    /// bindings that are guaranteed to be refs
    SETUP_REF,
    /// declared by other options, e.g. computed, inject
    OPTIONS,
    /// a literal constant, e.g. 'foo', 1, true
    LITERAL_CONST,

    // Introduced by fervid:
    /// a `.vue` import or `defineComponent` call
    COMPONENT,
    /// an import which is not a `.vue` or `from 'vue'`
    IMPORTED,
    /// a variable from the template
    TEMPLATE_LOCAL,
    /// a variable in the global Javascript context, e.g. `Array` or `undefined`
    JS_GLOBAL,
    /// a non-resolved variable, presumably from the global Vue context
    UNRESOLVED,
}

//
// OUTPUT Serialization
//

impl From<fervid::BindingTypes> for BindingTypes {
    fn from(value: fervid::BindingTypes) -> Self {
        match value {
            fervid::BindingTypes::Data => BindingTypes::DATA,
            fervid::BindingTypes::Props => BindingTypes::PROPS,
            fervid::BindingTypes::PropsAliased => BindingTypes::PROPS_ALIASED,
            fervid::BindingTypes::SetupLet => BindingTypes::SETUP_LET,
            fervid::BindingTypes::SetupConst => BindingTypes::SETUP_CONST,
            fervid::BindingTypes::SetupReactiveConst => BindingTypes::SETUP_REACTIVE_CONST,
            fervid::BindingTypes::SetupMaybeRef => BindingTypes::SETUP_MAYBE_REF,
            fervid::BindingTypes::SetupRef => BindingTypes::SETUP_REF,
            fervid::BindingTypes::Options => BindingTypes::OPTIONS,
            fervid::BindingTypes::LiteralConst => BindingTypes::LITERAL_CONST,
            fervid::BindingTypes::Component => BindingTypes::COMPONENT,
            fervid::BindingTypes::Imported => BindingTypes::IMPORTED,
            fervid::BindingTypes::TemplateLocal => BindingTypes::TEMPLATE_LOCAL,
            fervid::BindingTypes::JsGlobal => BindingTypes::JS_GLOBAL,
            fervid::BindingTypes::Unresolved => BindingTypes::UNRESOLVED,
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

//
// Input De-Serialization
//

impl From<FervidTransformAssetUrlsOptions> for TransformAssetUrlsConfigOptions {
    fn from(value: FervidTransformAssetUrlsOptions) -> TransformAssetUrlsConfigOptions {
        let tags = if let Some(napi_tags) = value.tags {
            let mut tags = FxHashMap::default();

            for mut tag in napi_tags {
                tags.insert(tag.0.into(), tag.1.drain(..).map(FervidAtom::from).collect());
            }

            tags
        } else {
            TransformAssetUrlsConfigOptions::default().tags
        };

        TransformAssetUrlsConfigOptions {
            base: value.base,
            include_absolute: value.include_absolute.unwrap_or_default(),
            tags,
        }
    }
}
