use swc_core::common::{Span, Spanned};

#[derive(Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

#[derive(Debug, PartialEq)]
pub enum ParseErrorKind {
    /// Malformed directive (e.g. `:`, `@`)
    DirectiveSyntax,
    /// Malformed directive name (e.g. `v-.`)
    DirectiveSyntaxDirectiveName,
    /// Malformed directive argument (e.g. `v-bind:`)
    DirectiveSyntaxArgument,
    /// Malformed directive dynamic argument (e.g. `:[foo`)
    DirectiveSyntaxDynamicArgument,
    /// Malformed directive - expected modifiers, but got something else (e.g. `:[foo]v`)
    DirectiveSyntaxUnexpectedCharacterAfterDynamicArgument,
    /// Malformed directive modifier (e.g. `:smth.`)
    DirectiveSyntaxModifier,
    /// More than one `<script>`
    DuplicateScriptOptions,
    /// More than one `<script setup>`
    DuplicateScriptSetup,
    /// More than one `<template>`
    DuplicateTemplate,
    /// More than one attribute with the same name on a root element
    DuplicateAttribute,
    /// Error while parsing EcmaScript/TypeScript
    EcmaSyntaxError(Box<swc_ecma_parser::error::SyntaxError>),
    /// Unrecoverable error while parsing HTML
    InvalidHtml(Box<swc_html_parser::error::ErrorKind>),
    /// Both `<template>` and `<script>` are missing
    MissingTemplateOrScript,
    /// `<script>`/`<style>` content was not Text
    UnexpectedNonRawTextContent,
    /// Language not supported
    UnsupportedLang,
    /// More than 3 aliases provided to `v-for` (e.g. `v-for="(value, key, index, extra) in list"`)
    VForTooManyAliasSlots,
    /// Value alias was empty (e.g. `v-for="() in list"`)
    VForMissingValueAlias,
    /// There was a comma after rest alias (e.g. `v-for="(value, ...rest,) in list"`)
    VForCommaAfterRestAlias,
    /// Edge case - there was a comma before `of` but no other delimiter
    /// (e.g. `v-for="value, of rows"`).
    /// Since `of` can be a valid identifier, Fervid considers it ambiguous input: it can be
    /// both `value of rows` or `value, of <missing delimiter - error> rows`.
    VForUnexpectedCommaBeforeOf,
    /// A closing parenthesis was missing (e.g. `v-for="(value in list"`)
    VForExpectedClosingAliasParenthesis,
    /// A delimiter was missing (e.g. `v-for="value list"`)
    VForExpectedDelimiter,
    /// No whitespace around delimiter (e.g. non-js-whitespace was used)
    VForDelimiterRequiresWhitespace,
    /// There were more tokens after source (e.g. `v-for="value in list extra"`)
    VForUnexpectedTokensAfterSource,
    /// Optional mark `?` was used incorrectly (e.g. `v-for="value, key = foo? in list"`)
    VForInvalidOptionalAlias,
    /// Optional mark `?` was used incorrectly on rest alias (e.g. `v-for="value, ...rest? in list"`)
    VForRestAliasCannotBeOptional,
    /// TS annotation was used incorrectly (e.g. `v-for="value, { key = foo } as Bar in list"`)
    VForExpectedBindingPattern,
    /// Default assignment was used incorrectly on rest alias (e.g. `v-for="value, ...rest = incorrect in list"`)
    VForRestAliasCannotHaveDefault,
    /// Default assignment was used incorrectly (e.g. `v-for="value? = incorrect in list"`)
    VForOptionalAliasCannotHaveDefault,
}

impl From<swc_ecma_parser::error::Error> for ParseError {
    fn from(value: swc_ecma_parser::error::Error) -> ParseError {
        let span = value.span();

        ParseError {
            kind: ParseErrorKind::EcmaSyntaxError(Box::new(value.into_kind())),
            span,
        }
    }
}

impl std::fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Spanned for ParseError {
    fn span(&self) -> Span {
        self.span
    }
}
