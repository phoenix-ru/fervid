use fervid_core::error::{Severity, SeverityLevel};
use swc_css_parser::error::Error as ParseError;

#[derive(Debug)]
pub enum CssError {
    ParseRecoverable(ParseError),
    ParseUnrecoverable(ParseError),
    ParseDeepRecoverable(ParseError),
    ParseDeepUnrecoverable(ParseError),
    // MinifyError(Error<MinifyErrorKind>),
    // PrinterError(Error<PrinterErrorKind>),
}

impl Severity for CssError {
    fn get_severity(&self) -> SeverityLevel {
        match self {
            CssError::ParseRecoverable(_) => SeverityLevel::RecoverableError,
            CssError::ParseUnrecoverable(_) => SeverityLevel::UnrecoverableError,
            CssError::ParseDeepRecoverable(_) => SeverityLevel::RecoverableError,
            CssError::ParseDeepUnrecoverable(_) => SeverityLevel::UnrecoverableError,
        }
    }
}
