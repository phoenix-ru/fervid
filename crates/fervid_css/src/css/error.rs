use fervid_core::error::{Severity, SeverityLevel};
use swc_core::common::{Span, Spanned};
use swc_css_parser::error::{Error as ParseError, ErrorKind as ParseErrorKind};

#[derive(Debug)]
pub struct CssError {
    pub span: Span,
    pub kind: CssErrorKind,
}

#[derive(Debug)]
pub enum CssErrorKind {
    ParseRecoverable(ParseErrorKind),
    ParseUnrecoverable(ParseErrorKind),
    ParseDeepRecoverable(ParseErrorKind),
    ParseDeepUnrecoverable(ParseErrorKind),
    // MinifyError(Error<MinifyErrorKind>),
    // PrinterError(Error<PrinterErrorKind>),
}

impl CssError {
    pub fn from_parse_error(from: ParseError, is_recoverable: bool, is_deep: bool) -> CssError {
        let (span, kind) = *from.into_inner();

        let kind = match (is_deep, is_recoverable) {
            (true, true) => CssErrorKind::ParseDeepRecoverable(kind),
            (true, false) => CssErrorKind::ParseDeepUnrecoverable(kind),
            (false, true) => CssErrorKind::ParseRecoverable(kind),
            (false, false) => CssErrorKind::ParseUnrecoverable(kind),
        };

        CssError { span, kind }
    }
}

impl Severity for CssError {
    fn get_severity(&self) -> SeverityLevel {
        match &self.kind {
            CssErrorKind::ParseRecoverable(_) => SeverityLevel::RecoverableError,
            CssErrorKind::ParseUnrecoverable(_) => SeverityLevel::UnrecoverableError,
            CssErrorKind::ParseDeepRecoverable(_) => SeverityLevel::RecoverableError,
            CssErrorKind::ParseDeepUnrecoverable(_) => SeverityLevel::UnrecoverableError,
        }
    }
}

impl Spanned for CssError {
    fn span(&self) -> swc_core::common::Span {
        self.span
    }
}
