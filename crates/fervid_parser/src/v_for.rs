use fervid_core::ForParseResult;
use swc_core::{
    common::{Span, comments::SingleThreadedComments},
    ecma::ast::{AssignOp, AssignPat, Pat, RestPat},
};
use swc_ecma_lexer::{
    Lexer, Parser, StringInput, Syntax,
    common::{
        input::Tokens,
        parser::{
            Parser as ParserTrait, buffer::Buffer as ParserBuffer, expr::parse_assignment_expr,
            pat::parse_binding_pat_or_ident, typescript::try_parse_ts_type_ann,
        },
    },
    token::{IdentLike, Keyword, KnownIdent, Token, TokenAndSpan, Word},
};

use crate::{ParseError, error::ParseErrorKind};

pub fn parse_v_for(
    raw: &str,
    span: Span,
    comments: &SingleThreadedComments,
) -> Result<ForParseResult, ParseError> {
    let lexer = Lexer::new(
        Syntax::Typescript(Default::default()),
        Default::default(),
        StringInput::new(raw, span.lo, span.hi),
        Some(comments),
    );
    finish(&mut Parser::new_from(lexer), raw, span)
}

fn finish<I: Tokens<TokenAndSpan>>(
    p: &mut Parser<I>,
    raw: &str,
    span: Span,
) -> Result<ForParseResult, ParseError> {
    let result = parse_body(p, raw, span).and_then(|result| {
        // Errors recovered by SWC are not accepted. This applies to cases like:
        // `/foo/gg` which has a duplicate RegEx flag. We require here that syntax is correct.
        if let Some(error) = p.take_errors().into_iter().next() {
            Err(ParseError::from(error))
        } else {
            Ok(result)
        }
    });
    result.map_err(|failure| ParseError {
        span: failure.span,
        kind: failure.kind,
    })
}

fn parse_body<I: Tokens<TokenAndSpan>>(
    p: &mut Parser<I>,
    raw: &str,
    span: Span,
) -> Result<ForParseResult, ParseError> {
    let parenthesized = p.input_mut().eat(&Token::LParen);
    let mut aliases = Vec::with_capacity(3);
    let mut last_comma = None;

    loop {
        if aliases.len() == 3 {
            return Err(fail(p, ParseErrorKind::VForTooManyAliasSlots));
        }
        let cur = p.input().cur();
        let empty = cur == &Token::Comma
            || (parenthesized && cur == &Token::RParen)
            || (!parenthesized && is_in_token(cur) && !aliases.is_empty());
        if empty {
            if aliases.is_empty() {
                return Err(fail(p, ParseErrorKind::VForMissingValueAlias));
            }
            aliases.push(None);
        } else {
            aliases.push(Some(parse_alias(p, parenthesized)?));
        }

        if p.input().cur() != &Token::Comma {
            break;
        }
        if aliases
            .last()
            .and_then(Option::as_deref)
            .is_some_and(Pat::is_rest)
        {
            return Err(fail(p, ParseErrorKind::VForCommaAfterRestAlias));
        }
        last_comma = Some(p.input().cur_span());
        p.input_mut().bump();
    }

    let at_alias_end = if parenthesized {
        p.input().cur() == &Token::RParen
    } else {
        is_delimiter_token(p.input().cur())
    };
    if !at_alias_end {
        // `value, of rows` and `(value, of rows)` parse `of` as another binding
        // We find the `,` before `of` and report it as an error.
        // Maybe we can simply recover from that and interpret as `(value, ) of rows`
        // but I'd argue this case causes ambiguity and should be resolved by user,
        // as it can be a genuine error that they wanted to add key but forgot.
        if let Some(comma) = last_comma {
            let last_is_plain_of = aliases
                .last()
                .and_then(Option::as_deref)
                .and_then(Pat::as_ident)
                .is_some_and(|ident| {
                    ident.id.sym == "of" && ident.type_ann.is_none() && !ident.id.optional
                });
            if last_is_plain_of {
                return Err(ParseError {
                    span: comma,
                    kind: ParseErrorKind::VForUnexpectedCommaBeforeOf,
                });
            }
        }
    }

    if parenthesized {
        expect(
            p,
            Token::RParen,
            ParseErrorKind::VForExpectedClosingAliasParenthesis,
        )?;
    }

    // Check delimiter and whitespace around it
    if !is_delimiter_token(p.input().cur()) {
        return Err(fail(p, ParseErrorKind::VForExpectedDelimiter));
    }
    let delimiter = p.input().cur_span();
    let lo = delimiter.lo.0.saturating_sub(span.lo.0) as usize;
    let hi = delimiter.hi.0.saturating_sub(span.lo.0) as usize;
    if !matches!(raw.get(lo..hi), Some("in" | "of"))
        || !raw
            .get(..lo)
            .and_then(|s| s.chars().next_back())
            .is_some_and(is_js_space)
        || !raw
            .get(hi..)
            .and_then(|s| s.chars().next())
            .is_some_and(is_js_space)
    {
        return Err(fail(p, ParseErrorKind::VForDelimiterRequiresWhitespace));
    }
    p.input_mut().bump();

    // Parse source as Expr
    let source = p.allow_in_expr(|p| p.parse_expr())?;
    if *p.input().cur() != Token::Eof {
        return Err(fail(p, ParseErrorKind::VForUnexpectedTokensAfterSource));
    }

    let mut aliases = aliases.into_iter();
    let Some(Some(value)) = aliases.next() else {
        return Err(fail(p, ParseErrorKind::VForMissingValueAlias));
    };
    Ok(ForParseResult {
        source,
        value,
        key: aliases.next().flatten(),
        index: aliases.next().flatten(),
        finalized: false,
        finalized_is_dynamic: false,
        source_const_type: Default::default(),
    })
}

fn parse_alias<I: Tokens<TokenAndSpan>>(
    p: &mut Parser<I>,
    parenthesized: bool,
) -> Result<Box<Pat>, ParseError> {
    let start = p.cur_pos();
    let rest = p.input_mut().eat(&Token::DotDotDot);
    let dot3_token = p.input().prev_span();

    // SWC builds nested destructuring, computed keys and nested defaults directly
    let mut pattern = parse_binding_pat_or_ident(p, false)?;
    let optional = p.input_mut().eat(&Token::QuestionMark);
    if optional {
        match &mut pattern {
            Pat::Ident(ident) => ident.id.optional = true,
            Pat::Array(array) => array.optional = true,
            Pat::Object(object) => object.optional = true,
            _ => return Err(fail(p, ParseErrorKind::VForInvalidOptionalAlias)),
        }
        if rest {
            return Err(fail(p, ParseErrorKind::VForRestAliasCannotBeOptional));
        }
    }

    // Delegate type grammar to SWC too, including generic/conditional/function types
    let type_ann = try_parse_ts_type_ann(p)?;
    let end = p.input().prev_span().hi;
    if rest {
        pattern = Pat::Rest(RestPat {
            span: Span::new(start, end),
            dot3_token,
            arg: Box::new(pattern),
            type_ann,
        });
    } else {
        match &mut pattern {
            Pat::Ident(ident) => ident.type_ann = type_ann,
            Pat::Array(array) => {
                array.type_ann = type_ann;
                array.span.hi = end;
            }
            Pat::Object(object) => {
                object.type_ann = type_ann;
                object.span.hi = end;
            }
            _ => return Err(fail(p, ParseErrorKind::VForExpectedBindingPattern)),
        }
    }

    if p.input_mut().eat(&Token::AssignOp(AssignOp::Assign)) {
        if rest {
            return Err(fail(p, ParseErrorKind::VForRestAliasCannotHaveDefault));
        }
        if optional {
            return Err(fail(p, ParseErrorKind::VForOptionalAliasCannotHaveDefault));
        }
        // Guide SWC parser about whether `in` should be consumed during parsing of assignment
        // expression, or not. We use the heuristic of alias being parenthesized - if it is,
        // let the parser consume `in`. If not - disallow so that it doesn't greedily consume
        // the `in` before the source.

        // value = key in object in rows
        // └─ alias ─┘    └─ source ───┘

        // (value = key in object) in rows
        //  └────── alias ───────┘    └source
        let right = if parenthesized {
            p.allow_in_expr(parse_assignment_expr)?
        } else {
            p.disallow_in_expr(parse_assignment_expr)?
        };
        pattern = Pat::Assign(AssignPat {
            span: Span::new(start, p.input().prev_span().hi),
            left: Box::new(pattern),
            right,
        });
    }
    // Stop on recovered alias errors before doing unrelated source work
    if let Some(error) = p.take_errors().into_iter().next() {
        return Err(ParseError::from(error));
    }
    Ok(Box::new(pattern))
}

/// ECMAScript White Space (https://tc39.es/ecma262/#sec-white-space): TAB, VT, FF, BOM, Unicode Space_Separator category
/// ECMAScript Line Terminators (https://tc39.es/ecma262/#sec-line-terminators): LF, CR, U+2028, U+2029
fn is_js_space(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\n' | '\r' | '\t' | '\u{b}' | '\u{c}' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                // cspell:disable-next-line
                | '\u{feff}'
    )
}

fn is_in_token(token: &Token) -> bool {
    matches!(token, Token::Word(Word::Keyword(Keyword::In)))
}

fn is_of_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Word(Word::Ident(IdentLike::Known(KnownIdent::Of)))
    )
}

fn is_delimiter_token(token: &Token) -> bool {
    is_in_token(token) || is_of_token(token)
}

fn expect<I: Tokens<TokenAndSpan>>(
    p: &mut Parser<I>,
    token: Token,
    kind: ParseErrorKind,
) -> Result<(), ParseError> {
    if p.input_mut().eat(&token) {
        Ok(())
    } else {
        Err(fail(p, kind))
    }
}

fn fail<I: Tokens<TokenAndSpan>>(p: &Parser<I>, kind: ParseErrorKind) -> ParseError {
    ParseError {
        span: p.input().cur_span(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use swc_core::common::{BytePos, Spanned};

    use super::*;

    #[test]
    fn it_parses_v_for_value() {
        macro_rules! check {
            ($input:expr, $source:expr, $value:expr, $key:expr, $index:expr) => {
                check_v_for_value($input, $source, $value, $key, $index);
            };
        }

        check!("item in list", "list", "item", None, None);
        check!("item of list", "list", "item", None, None);
        check!("i in 3", "3", "i", None, None);
        check!("(item, index) in list", "list", "item", Some("index"), None);
        check!(
            "(value, key, index) of object",
            "object",
            "value",
            Some("key"),
            Some("index")
        );
        check!(
            "([item1, item2], index) in list",
            "list",
            "[item1, item2]",
            Some("index"),
            None
        );
        check!(
            "(value, , index) in object",
            "object",
            "value",
            None,
            Some("index")
        );
        check!("   item   in \n \t  list   ", "list", "item", None, None);
    }

    // Additional Fervid parser cases.
    // vuejs-core regex splitter mishandles some of those, especially `in` being a part of alias,
    // e.g. `({ present = key in object }, index) in rows`
    mod v_for_value_edge_cases {
        use super::*;

        // Auto-generated test cases
        macro_rules! check {
            ($name:ident, $input:expr, $source:expr, $value:expr, $key:expr, $index:expr) => {
                #[test]
                fn $name() {
                    check_v_for_value($input, $source, $value, $key, $index);
                }
            };
        }

        check!(
            ts_default_and_source_assertion,
            "{ z = x as Qux } in list as Fred",
            "list as Fred",
            "{ z = x as Qux }",
            None,
            None
        );
        check!(
            renamed_property_default,
            "({ q: Foo = x }, key, index) in list",
            "list",
            "{ q: Foo = x }",
            Some("key"),
            Some("index")
        );
        check!(
            nested_patterns_and_rest,
            "({ user: { name = fallback }, items: [first, , ...rest], ...other }, key, index) of records",
            "records",
            "{ user: { name = fallback }, items: [first, , ...rest], ...other }",
            Some("key"),
            Some("index")
        );
        check!(
            array_holes_and_nested_defaults,
            "([, { x = make(1, 2) }, ...rest], , index) in rows",
            "rows",
            "[, { x = make(1, 2) }, ...rest]",
            None,
            Some("index")
        );
        check!(
            computed_property_and_sequence_default,
            "({ [lookup(a, b)]: value = (x, y) }, key) in rows",
            "rows",
            "{ [lookup(a, b)]: value = (x, y) }",
            Some("key"),
            None
        );
        check!(
            function_default_and_parameter_scope,
            "({ handler = function (x = name, y = x, z = function(v) { return y + bar - v }, v = foo) {} }, key) in rows",
            "rows",
            "{ handler = function (x = name, y = x, z = function(v) { return y + bar - v }, v = foo) {} }",
            Some("key"),
            None
        );
        check!(
            generic_call_default,
            "({ z = make<Foo, Bar>(x, y) }, key, index) in list as Fred",
            "list as Fred",
            "{ z = make<Foo, Bar>(x, y) }",
            Some("key"),
            Some("index")
        );
        check!(
            complex_source,
            "(value, key, index) in getRows(a, b).filter((row, i) => i in row)",
            "getRows(a, b).filter((row, i) => i in row)",
            "value",
            Some("key"),
            Some("index")
        );
        // Translation: \u{2003}( Value, Key, Number ) of \u{a0} List\u{2003}
        check!(
            unicode_aliases_and_whitespace,
            "\u{2003}( 値 , 鍵 , 番号 ) of \u{a0}一覧\u{2003}",
            "一覧",
            "値",
            Some("鍵"),
            Some("番号")
        );
        check!(
            single_parenthesized_alias,
            " ( { z = x } ) in list ",
            "list",
            "{ z = x }",
            None,
            None
        );
        check!(
            omitted_key_and_index,
            "(value, , ) in object",
            "object",
            "value",
            None,
            None
        );
        check!(
            quoted_closing_delimiter,
            "({ text = '},', other = fallback }, key, index) in rows",
            "rows",
            "{ text = '},', other = fallback }",
            Some("key"),
            Some("index")
        );
        check!(
            quoted_opening_delimiter,
            "({ text = '(' }, key, index) in rows",
            "rows",
            "{ text = '(' }",
            Some("key"),
            Some("index")
        );
        check!(
            double_quoted_delimiter,
            r#"({ text = "}," }, key, index) in rows"#,
            "rows",
            r#"{ text = "}," }"#,
            Some("key"),
            Some("index")
        );
        check!(
            escaped_quote_and_delimiter,
            r#"({ text = 'it\'s },' }, key, index) in rows"#,
            "rows",
            r#"{ text = 'it\'s },' }"#,
            Some("key"),
            Some("index")
        );
        check!(
            template_literal_default,
            "({ text = `}, ${format(x, y)}` }, key, index) in rows",
            "rows",
            "{ text = `}, ${format(x, y)}` }",
            Some("key"),
            Some("index")
        );
        check!(
            regex_default,
            "({ matcher = /[},]/ }, key, index) in rows",
            "rows",
            "{ matcher = /[},]/ }",
            Some("key"),
            Some("index")
        );
        check!(
            regex_escaped_opening_delimiter,
            r"({ matcher = /\(/ }, key, index) in rows",
            "rows",
            r"{ matcher = /\(/ }",
            Some("key"),
            Some("index")
        );
        check!(
            block_comment_delimiters,
            "({ z = /* }, */ fallback }, key, index) in rows",
            "rows",
            "{ z = /* }, */ fallback }",
            Some("key"),
            Some("index")
        );
        check!(
            line_comment_delimiters,
            "({ z = fallback // },\n }, key, index) in rows",
            "rows",
            "{ z = fallback // },\n }",
            Some("key"),
            Some("index")
        );
        check!(
            in_operator_in_default,
            "({ present = key in object }, index) in rows",
            "rows",
            "{ present = key in object }",
            Some("index"),
            None
        );
        check!(
            in_text_in_default,
            "({ text = ' in ' }, key) in rows",
            "rows",
            "{ text = ' in ' }",
            Some("key"),
            None
        );
        check!(
            of_text_in_default,
            "({ text = ' of ' }, key) of rows",
            "rows",
            "{ text = ' of ' }",
            Some("key"),
            None
        );
        check!(
            of_source_containing_in,
            "item of rows.filter(row => key in row)",
            "rows.filter(row => key in row)",
            "item",
            None,
            None
        );
        check!(tabs_around_in, "item\tin\trows", "rows", "item", None, None);
        check!(
            newlines_around_of,
            "(value, key)\nof\nrows",
            "rows",
            "value",
            Some("key"),
            None
        );
    }

    fn check_v_for_value(
        input: &str,
        source: &str,
        value: &str,
        key: Option<&str>,
        index: Option<&str>,
    ) {
        for offset in [0, 1, 137] {
            let parsed = parse_v_for(
                input,
                Span::new(BytePos(offset), BytePos(offset + input.len() as u32)),
                &Default::default(),
            )
            .unwrap_or_else(|error| panic!("failed {input:?}: {error:?}"));
            let slice =
                |span: Span| &input[(span.lo.0 - offset) as usize..(span.hi.0 - offset) as usize];
            assert_eq!(source, slice(parsed.source.span()), "source in {input:?}");
            assert_eq!(value, slice(parsed.value.span()), "value in {input:?}");
            assert_eq!(
                key,
                parsed.key.map(|pat| slice(pat.span())),
                "key in {input:?}"
            );
            assert_eq!(
                index,
                parsed.index.map(|pat| slice(pat.span())),
                "index in {input:?}"
            );
        }
    }

    #[test]
    fn typed_holes_and_nested_generics() {
        check_v_for_value(
            "(value: Map<Foo, Bar>, , index: number) in rows",
            "rows",
            "value: Map<Foo, Bar>",
            None,
            Some("index: number"),
        );
    }

    #[test]
    fn of_is_a_valid_binding_name_and_a_contextual_delimiter() {
        check_v_for_value("of in rows", "rows", "of", None, None);
        check_v_for_value("of of rows", "rows", "of", None, None);
        check_v_for_value("value, of in rows", "rows", "value", Some("of"), None);
        check_v_for_value("value, of of rows", "rows", "value", Some("of"), None);
        check_v_for_value("(value, ) of rows", "rows", "value", None, None);
    }

    #[test]
    fn bare_comma_before_of_reports_unexpected_comma() {
        check_unexpected_comma("value, of rows");
    }

    #[test]
    fn parenthesized_comma_before_of_reports_unexpected_comma() {
        check_unexpected_comma("(value, of rows)");
    }

    #[test]
    fn of_binding_with_missing_delimiter_keeps_generic_error() {
        let comments = Default::default();

        for raw in ["of rows", "(value, of) rows", "value, of: Key rows"] {
            let error = parse_v_for(
                raw,
                Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
                &comments,
            )
            .expect_err("missing delimiter");
            assert_eq!(error.kind, ParseErrorKind::VForExpectedDelimiter, "{raw}");
        }
    }

    #[test]
    fn root_assignment_has_explicit_no_in_policy() {
        check_v_for_value(
            "value = key in object in rows",
            "object in rows",
            "value = key",
            None,
            None,
        );
        check_v_for_value(
            "(value = key in object) in rows",
            "rows",
            "value = key in object",
            None,
            None,
        );
    }

    #[test]
    fn no_in_is_grammar_context() {
        check_v_for_value(
            "value = test ? key in object : fallback in rows",
            "rows",
            "value = test ? key in object : fallback",
            None,
            None,
        );
        check_v_for_value(
            "value = (key in object) in rows",
            "rows",
            "value = (key in object)",
            None,
            None,
        );
    }

    #[test]
    fn arrow_default_ambiguity_uses_same_explicit_policy() {
        check_v_for_value(
            "value = x => x in object in rows",
            "object in rows",
            "value = x => x",
            None,
            None,
        );
        check_v_for_value(
            "(value = x => x in object) in rows",
            "rows",
            "value = x => x in object",
            None,
            None,
        );
    }

    #[test]
    fn comment_only_omission_and_trailing_comments() {
        let comments = Default::default();

        let raw = "(value, /* omitted, in here */ , index) in rows // tail";
        let parsed = parse_v_for(
            raw,
            Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
            &comments,
        )
        .expect("valid comments");
        assert!(parsed.key.is_none());
        assert!(parsed.index.is_some());
        assert!(parsed.source.is_ident());
    }

    #[test]
    fn errors_stop_without_retrying_other_separators() {
        let raw = format!("({{ x = @ {} }}, key) in rows", " in ".repeat(1000));
        let error = parse_v_for(
            &raw,
            Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
            &Default::default(),
        )
        .expect_err("bad default");
        assert!(matches!(error.kind, ParseErrorKind::EcmaSyntaxError(_)))
    }

    #[test]
    fn rejects_prefix_success_and_malformed_aliases() {
        let comments = Default::default();

        for raw in [
            "item in rows garbage",
            "(item.field, key) in rows",
            "(a,b,c,d) in rows",
            "(, key) in rows",
            "item in ",
            "item rows",
            "{ x: } in rows",
            "({ x = }, key) in rows",
            "(...items, key) in rows",
            "item in rows; other",
            "(item, key junk) in rows",
            "(item => item, key) in rows",
            "(item)=>{}// in rows",
            "item in[1]",
            "(item? = x) in rows",
        ] {
            assert!(
                parse_v_for(
                    raw,
                    Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
                    &comments
                )
                .is_err(),
                "accepted {raw:?}"
            );
        }
    }

    #[test]
    fn parse_pat_is_not_a_complete_parameter_parser() {
        let raw = "item = fallback";
        let lexer = Lexer::new(
            Syntax::Typescript(Default::default()),
            Default::default(),
            StringInput::new(raw, BytePos(1), BytePos(1 + raw.len() as u32)),
            None,
        );
        let mut p = Parser::new_from(lexer);
        assert!(
            parse_binding_pat_or_ident(&mut p, false)
                .expect("binding identifier")
                .is_ident()
        );
        assert_eq!(
            p.input().cur(),
            &Token::AssignOp(AssignOp::Assign),
            "root initializer remains unread"
        );
    }

    #[test]
    fn recovered_pattern_error_can_return_ast_before_delimiter() {
        use swc_ecma_parser::error::SyntaxError;

        let raw = "{ ...rest, after } in rows";
        let lexer = Lexer::new(
            Syntax::Typescript(Default::default()),
            Default::default(),
            StringInput::new(raw, BytePos(1), BytePos(1 + raw.len() as u32)),
            None,
        );
        let mut p = Parser::new_from(lexer);
        assert!(
            parse_binding_pat_or_ident(&mut p, false)
                .expect("SWC recovers non-final object rest")
                .is_object()
        );
        assert_eq!(p.input().cur(), &Token::Word(Word::Keyword(Keyword::In)));
        assert!(
            p.take_errors()
                .iter()
                .any(|error| matches!(error.kind(), SyntaxError::NonLastRestParam))
        );

        let error = parse_v_for(
            raw,
            Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
            &Default::default(),
        )
        .expect_err("recovered alias error invalidates result");
        assert_eq!(
            error.kind,
            ParseErrorKind::EcmaSyntaxError(Box::new(SyntaxError::NonLastRestParam))
        );
    }

    #[test]
    fn recovered_source_error_can_return_ast_at_eof() {
        use swc_ecma_parser::error::SyntaxError;

        let source = "/x/gg";
        let lexer = Lexer::new(
            Syntax::Typescript(Default::default()),
            Default::default(),
            StringInput::new(source, BytePos(1), BytePos(1 + source.len() as u32)),
            None,
        );
        let mut p = Parser::new_from(lexer);
        assert!(
            p.parse_expr()
                .expect("SWC recovers duplicate regex flags")
                .is_lit()
        );
        assert_eq!(p.input().cur(), &Token::Eof);
        assert!(
            p.take_errors()
                .iter()
                .any(|error| matches!(error.kind(), SyntaxError::DuplicatedRegExpFlags('g')))
        );

        let raw = "item in /x/gg";
        let error = parse_v_for(
            raw,
            Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
            &Default::default(),
        )
        .expect_err("EOF alone does not make a recovered expression valid");
        assert_eq!(
            error.kind,
            ParseErrorKind::EcmaSyntaxError(Box::new(SyntaxError::DuplicatedRegExpFlags('g'))),
        );
    }

    #[test]
    fn fatal_pattern_error_returns_err_before_source() {
        let raw = "{ x = @ } in rows";
        let lexer = Lexer::new(
            Syntax::Typescript(Default::default()),
            Default::default(),
            StringInput::new(raw, BytePos(1), BytePos(1 + raw.len() as u32)),
            None,
        );
        let mut p = Parser::new_from(lexer);
        assert!(parse_binding_pat_or_ident(&mut p, false).is_err());
        assert_eq!(p.input().cur(), &Token::At);
        let error = parse_v_for(
            raw,
            Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
            &Default::default(),
        )
        .expect_err("fatal alias error");
        assert!(matches!(error.kind, ParseErrorKind::EcmaSyntaxError(_)))
    }

    #[test]
    fn default_ts_ast_and_nested_spans_are_retained() {
        let raw = "{ z = x as Qux } in list as Fred";
        let parsed = parse_v_for(
            raw,
            Span::new(BytePos(137), BytePos(137 + raw.len() as u32)),
            &Default::default(),
        )
        .expect("valid TypeScript");
        let default = default_expression(&parsed.value);
        let assertion = default.as_ts_as().expect("default TS assertion");
        assert!(assertion.expr.as_ident().is_some_and(|id| id.sym == "x"));
        assert_eq!(
            default.span().lo.0,
            137 + raw.find("x as Qux").expect("default text") as u32
        );
        assert_eq!(
            default.span().hi.0 - default.span().lo.0,
            "x as Qux".len() as u32
        );
    }

    #[test]
    fn root_type_default_and_rest() {
        check_v_for_value(
            "(value: Foo = fallback, key, ...index: number[]) in rows",
            "rows",
            "value: Foo = fallback",
            Some("key"),
            Some("...index: number[]"),
        );
        check_v_for_value(
            "(value?: Foo, key?: string) in rows",
            "rows",
            "value?: Foo",
            Some("key?: string"),
            None,
        );
    }

    #[test]
    fn nested_templates_and_division_do_not_confuse_boundaries() {
        check_v_for_value(
            "({ x = `, ${` in ${fn(a, b)}`}` , r = /a,b/, n = a / b / c }, key) of rows",
            "rows",
            "{ x = `, ${` in ${fn(a, b)}`}` , r = /a,b/, n = a / b / c }",
            Some("key"),
            None,
        );
    }

    #[test]
    fn line_comment_delimiters_remain_trivia() {
        let raw = "item // , key in ignored\n in rows // tail";
        let parsed = parse_v_for(
            raw,
            Span::new(BytePos(1), BytePos(1 + raw.len() as u32)),
            &Default::default(),
        )
        .expect("comment contents must not become delimiters");
        assert!(parsed.key.is_none());
        assert!(parsed.source.as_ident().is_some_and(|id| id.sym == "rows"));
    }

    fn check_unexpected_comma(raw: &str) {
        for offset in [0, 137] {
            let error = parse_v_for(
                raw,
                Span::new(BytePos(offset), BytePos(offset + raw.len() as u32)),
                &Default::default(),
            )
            .expect_err("stray comma before apparent delimiter");
            let comma = offset + raw.find(',').expect("comma in input") as u32;
            assert_eq!(error.kind, ParseErrorKind::VForUnexpectedCommaBeforeOf);
            assert_eq!(error.span, Span::new(BytePos(comma), BytePos(comma + 1)));
        }
    }

    fn default_expression(pat: &Pat) -> &swc_core::ecma::ast::Expr {
        let object = pat.as_object().expect("object pattern");
        let swc_core::ecma::ast::ObjectPatProp::Assign(property) =
            object.props.first().expect("default property")
        else {
            panic!("expected default property")
        };
        property.value.as_deref().expect("default expression")
    }
}
