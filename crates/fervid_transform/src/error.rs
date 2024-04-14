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
    /// Disallow non-type exports inside `<script setup>`
    SetupExport
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
