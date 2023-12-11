pub trait Severity {
    fn get_severity(&self) -> SeverityLevel;

    /// Returns `true` if the severity level is [`RecoverableError`].
    ///
    /// [`RecoverableError`]: SeverityLevel::RecoverableError
    #[must_use]
    fn is_recoverable_error(&self) -> bool {
        matches!(self.get_severity(), SeverityLevel::RecoverableError)
    }

    /// Returns `true` if the severity level is [`UnrecoverableError`].
    ///
    /// [`UnrecoverableError`]: SeverityLevel::UnrecoverableError
    #[must_use]
    fn is_unrecoverable_error(&self) -> bool {
        matches!(self.get_severity(), SeverityLevel::UnrecoverableError)
    }

    /// Returns `true` if the severity level is [`Warning`].
    ///
    /// [`Warning`]: SeverityLevel::Warning
    #[must_use]
    fn is_warning(&self) -> bool {
        matches!(self.get_severity(), SeverityLevel::Warning)
    }
}

#[derive(Debug, PartialEq, Eq)]
#[allow(unused)]
pub enum SeverityLevel {
    UnrecoverableError,
    RecoverableError,
    Warning,
}
