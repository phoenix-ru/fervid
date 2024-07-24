//! Error definitions for the glue code of `fervid`

use fervid_parser::ParseError as SfcParseError;
use fervid_transform::error::TransformError;
use swc_core::common::Spanned;

#[derive(Debug)]
pub enum CompileError {
    /// An error occurred during the parsing of an SFC.
    ///
    /// This can be due to:
    /// - bad HTML;
    /// - bad ES/TS in bindings;
    /// - duplicate root blocks (e.g. `<template>`);
    /// - invalid directive syntax;
    /// - unclosed dynamic arguments (`:[dynamic`);
    /// - etc. etc.
    SfcParse(SfcParseError),

    /// An error during the transformation of an SFC.
    TransformError(TransformError)
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<SfcParseError> for CompileError {
    fn from(value: SfcParseError) -> Self {
        Self::SfcParse(value)
    }
}

impl From<TransformError> for CompileError {
    fn from(value: TransformError) -> Self {
        Self::TransformError(value)
    }
}

impl Spanned for CompileError {
    fn span(&self) -> swc_core::common::Span {
        match self {
            CompileError::SfcParse(e) => e.span,
            CompileError::TransformError(e) => e.span()
        }
    }
}
