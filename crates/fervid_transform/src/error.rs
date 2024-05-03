use fervid_css::CssError;
use swc_core::common::{Span, Spanned};

#[derive(Debug)]
pub struct TransformError {
    pub span: Span,
    pub kind: TransformErrorKind
}

#[derive(Debug)]
pub enum TransformErrorKind {
    CssError(CssError),
    ScriptError(ScriptError)
}

#[derive(Debug)]
pub enum ScriptError {
    /// A compiler macro was imported, but it didn't need to
    CompilerMacroImport,
    /// Different imports using the same local symbol,
    /// e.g `import foo from './foo'` and `import { foo } from './bar'`.
    DuplicateImport,
    /// Disallow non-type exports inside `<script setup>`
    SetupExport,
}

impl From<CssError> for TransformError {
    fn from(value: CssError) -> Self {
        TransformError {
            span: value.span(),
            kind: TransformErrorKind::CssError(value)
        }
    }
}

impl Spanned for TransformError {
    fn span(&self) -> Span {
        self.span
    }
}
