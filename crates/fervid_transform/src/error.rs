use fervid_css::CssError;
use swc_core::common::{Span, Spanned};

#[derive(Debug)]
pub enum TransformError {
    CssError(CssError),
    ScriptError(ScriptError),
    TemplateError(TemplateError),
}

#[derive(Debug)]
pub struct ScriptError {
    pub span: Span,
    pub kind: ScriptErrorKind,
}

#[derive(Debug)]
pub struct TemplateError {
    pub span: Span,
    pub kind: TemplateErrorKind,
}

#[derive(Debug)]
pub enum ScriptErrorKind {
    /// A compiler macro was imported, but it didn't need to
    CompilerMacroImport,
    /// `defineEmits` called with 0 type arguments (e.g. `defineEmits<>()`)
    DefineEmitsMalformed,
    /// `defineEmits` was called with both runtime and type arguments
    DefineEmitsTypeAndNonTypeArguments,
    /// "defineEmits() type cannot mixed call signature and property syntax"
    DefineEmitsMixedCallAndPropertySyntax,
    /// `defineProps` was called with both runtime and type arguments
    DefinePropsTypeAndNonTypeArguments,
    /// "`defineOptions` cannot accept type arguments"
    DefineOptionsTypeArguments,
    /// "`defineOptions` cannot be used to declare props. Use defineProps() instead."
    DefineOptionsProps,
    /// "`defineOptions` cannot be used to declare emits. Use defineEmits() instead."
    DefineOptionsEmits,
    /// "`defineOptions` cannot be used to declare expose. Use defineExpose() instead."
    DefineOptionsExpose,
    /// "`defineOptions` cannot be used to declare slots. Use defineSlots() instead."
    DefineOptionsSlots,
    /// `Props destructure is explicitly prohibited via config.`
    DefinePropsDestructureForbidden,
    /// `Props destructure cannot use computed key.`
    DefinePropsDestructureCannotUseComputedKey,
    /// `Cannot assign to destructured props as they are readonly.`
    DefinePropsDestructureCannotAssignToReadonly,
    /// `Destructured prop should not be passed directly to toRef(). Pass a getter instead`
    DefinePropsDestructureShouldNotPassToToRef,
    /// `Destructured prop should not be passed directly to watch(). Pass a getter instead`
    DefinePropsDestructureShouldNotPassToWatch,
    /// `Default value of prop does not match declared type.`
    DefinePropsDestructureDeclaredTypeMismatch,
    /// `withDefaults() is unnecessary when using destructure with defineProps().\nReactive destructure will be disabled when using withDefaults().\nPrefer using destructure default values, e.g. const { foo = 1 } = defineProps(...).`
    DefinePropsDestructureUnnecessaryWithDefaults,
    /// `Props destructure does not support nested patterns.`
    DefinePropsDestructureUnsupportedNestedPattern,
    /// "`defineSlots` cannot accept arguments"
    DefineSlotsArguments,
    /// Duplicate `defineEmits` call
    DuplicateDefineEmits,
    /// Duplicate `defineModel` model name
    DuplicateDefineModelName,
    /// Duplicate `defineProps` call
    DuplicateDefineProps,
    /// Duplicate `defineOptions` call
    DuplicateDefineOptions,
    /// Duplicate `defineSlots` call
    DuplicateDefineSlots,
    /// Different imports using the same local symbol,
    /// e.g `import foo from './foo'` and `import { foo } from './bar'`.
    DuplicateImport,
    /// Could not resolve array element type
    ResolveTypeElementType,
    /// "Failed to resolve extends base type"
    ResolveTypeExtendsBaseType,
    /// A type param was not provided,
    /// e.g. `ExtractPropTypes<>`
    ResolveTypeMissingTypeParam,
    /// Type parameters were not provided,
    /// e.g. `ExtractPropTypes`
    ResolveTypeMissingTypeParams,
    /// A type both not supported and not planned to be supported during type resolution
    ResolveTypeUnresolvable,
    /// "Failed to resolve index type into finite keys"
    ResolveTypeUnresolvableIndexType,
    /// An unsupported construction during type resolution
    ResolveTypeUnsupported,
    /// "Unsupported type when resolving index type"
    ResolveTypeUnsupportedIndexType,
    /// Unsupported computed key in type referenced by a macro
    ResolveTypeUnsupportedComputedKey,
    /// Disallow non-type exports inside `<script setup>`
    SetupExport,
    /// `withDefaults` only works with type-only `defineProps`
    WithDefaultsNeedsTypeOnlyDefineProps,
    /// `withDefaults` without `defineProps` inside
    WithDefaultsWithoutDefineProps,
}

#[derive(Debug)]
pub enum TemplateErrorKind {
    /// Failed parsing the URL when doing asset URL transform
    TransformAssetUrlsBaseUrlParseFailed,
    /// Failed parsing the configured base URL when doing asset URL transform
    TransformAssetUrlsUrlParseFailed,
}

impl From<CssError> for TransformError {
    fn from(value: CssError) -> Self {
        TransformError::CssError(value)
    }
}

impl From<ScriptError> for TransformError {
    fn from(value: ScriptError) -> Self {
        TransformError::ScriptError(value)
    }
}

impl Spanned for TransformError {
    fn span(&self) -> Span {
        match self {
            TransformError::CssError(e) => e.span,
            TransformError::ScriptError(e) => e.span,
            TransformError::TemplateError(e) => e.span,
        }
    }
}
