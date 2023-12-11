use swc_core::common::{input::StringInput, Span};
use swc_css_ast::{ComplexSelector, Stylesheet};
use swc_css_parser::{
    self,
    error::Error as ParseError,
    parse_string_input,
    parser::{PResult, ParserConfig},
};

/// Parses the `input` as `Stylesheet`
pub fn parse_stylesheet(
    input: &str,
    span: Span,
    config: ParserConfig,
    errors: &mut Vec<ParseError>,
) -> PResult<Stylesheet> {
    let parser_input = StringInput::new(input, span.lo, span.hi);
    parse_string_input(parser_input, None, config, errors)
}

/// Parses the `input` as `ComplexSelector`. This is needed for parsing `:deep`
pub fn parse_complex_selector(
    input: &str,
    span: Span,
    errors: &mut Vec<ParseError>,
) -> PResult<ComplexSelector> {
    let parser_input = StringInput::new(input, span.lo, span.hi);
    parse_string_input(parser_input, None, ParserConfig::default(), errors)
}

#[cfg(test)]
mod tests {
    use swc_core::common::{BytePos, SyntaxContext};

    use super::*;

    #[test]
    fn it_parses_regular() {
        assert_no_errors(".foo > #bar baz, .foo .bar { background: yellow }");
        assert_no_errors(".foo :deep(#bar baz), .qux { background: yellow }");
    }

    #[test]
    fn it_parses_tailwind() {
        assert_no_errors(
            "
            .hello-input__input {
                @apply br-p-1;
            }

            @screen sm {
                .hello-input__input:hover, .hello-input__input:focus {
                    border-color: #000;
                }
            }",
        );
    }

    fn assert_no_errors(input: &str) {
        let (parsed, errors) = test_parse(input);
        assert!(parsed.is_ok());
        assert!(errors.is_empty());
    }

    fn test_parse(input: &str) -> (Result<Stylesheet, ParseError>, Vec<ParseError>) {
        let span = Span::new(
            BytePos(1),
            BytePos(1 + input.len() as u32),
            SyntaxContext::default(),
        );
        let mut errors = Vec::new();
        let parsed = parse_stylesheet(input, span, ParserConfig::default(), &mut errors);

        (parsed, errors)
    }
}
