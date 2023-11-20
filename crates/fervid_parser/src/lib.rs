mod attributes;
mod error;
mod script;
mod sfc;
mod template;

use error::ParseError;

// Default patterns for interpolation
pub const INTERPOLATION_START_PAT_DEFAULT: &str = "{{";
pub const INTERPOLATION_END_PAT_DEFAULT: &str = "}}";

#[derive(Debug)]
pub struct SfcParser<'i, 'e, 'p> {
    input: &'i str,
    errors: &'e mut Vec<ParseError>,
    is_pre: bool,
    interpolation_start_pat: &'p str,
    interpolation_end_pat: &'p str,
}

impl<'i, 'e> SfcParser<'i, 'e, 'static> {
    pub fn new(input: &'i str, errors: &'e mut Vec<ParseError>) -> Self {
        // TODO When should it fail? What do we do with errors?..
        // I was thinking of 4 strategies:
        // `HARD_FAIL_ON_ERROR` (any error = fail (incl. recoverable), stop at the first error),
        // `SOFT_REPORT_ALL` (any error = fail (incl. recoverable), continue as far as possible and report after),
        // `SOFT_RECOVER_SAFE` (try to ignore recoverable, report the rest),
        // `SOFT_RECOVER_UNSAFE` (ignore as much as is possible, but still report).

        SfcParser {
            input,
            errors,
            is_pre: false,
            interpolation_start_pat: INTERPOLATION_START_PAT_DEFAULT,
            interpolation_end_pat: INTERPOLATION_END_PAT_DEFAULT,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::SfcParser;

    #[test]
    fn it_works() {
        let document = include_str!("../../fervid/benches/fixtures/input.vue");

        let mut errors = Vec::new();

        // Cold
        let now = std::time::Instant::now();
        let mut parser = SfcParser::new(document, &mut errors);
        let _ = parser.parse_sfc();
        let elapsed = now.elapsed();
        println!("Elapsed: {:?}", elapsed);
        parser.errors.clear();

        // Hot
        let now = std::time::Instant::now();
        let parsed = parser.parse_sfc();
        let elapsed = now.elapsed();
        println!("Elapsed: {:?}", elapsed);

        println!("{:#?}", parsed);

        for error in errors {
            let e = error;
            println!(
                "{:?} {}",
                e.kind,
                &document[e.span.lo.0 as usize - 1..e.span.hi.0 as usize - 1]
            );
        }

        // assert_eq!(errors.len(), 0);
    }
}
