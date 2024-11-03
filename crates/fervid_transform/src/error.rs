use fervid_css::CssError;
use swc_core::common::{Span, Spanned};

#[derive(Debug)]
pub enum TransformError {
    CssError(CssError),
    ScriptError(ScriptError)
}

#[derive(Debug)]
pub struct ScriptError {
    pub span: Span,
    pub kind: ScriptErrorKind
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
    /// Duplicate `defineEmits` call
    DuplicateDefineEmits,
    /// Duplicate `defineProps` call
    DuplicateDefineProps,
    /// Duplicate `defineOptions` call
    DuplicateDefineOptions,
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
        }
    }
}
