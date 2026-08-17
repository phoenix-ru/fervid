use std::borrow::Cow;

use fervid_core::{
    AttributeOrBinding, FervidAtom, ForParseResult, StrOrExpr, VBindDirective, VCustomDirective,
    VForDirective, VModelDirective, VOnDirective, VSlotDirective, VueDirectives,
};
use swc_core::{
    common::{BytePos, Span},
    ecma::ast::Str,
};
use swc_ecma_parser::Syntax;
use swc_html_ast::Attribute;

use crate::{
    SfcParser,
    error::{ParseError, ParseErrorKind},
};

impl SfcParser<'_, '_, '_> {
    /// Returns `true` when `v-pre` is discovered
    pub fn process_element_attributes(
        &mut self,
        raw_attributes: Vec<Attribute>,
        attrs_or_bindings: &mut Vec<AttributeOrBinding>,
        vue_directives: &mut Option<Box<VueDirectives>>,
    ) -> bool {
        // Skip any kind of processing for `v-pre` mode
        if self.is_pre {
            attrs_or_bindings.extend(raw_attributes.into_iter().map(create_regular_attribute));
            return false;
        }

        // Check existence of `v-pre` in attributes
        let has_v_pre = raw_attributes.iter().any(|attr| attr.name == "v-pre");
        if has_v_pre {
            attrs_or_bindings.extend(
                raw_attributes
                    .into_iter()
                    .filter(|attr| attr.name != "v-pre")
                    .map(create_regular_attribute),
            );
            return true;
        }

        for mut raw_attribute in raw_attributes.into_iter() {
            // Use raw names for attributes, otherwise SWC transforms them to lowercase
            // `-1` is needed because SWC spans start from 1
            let raw_idx_start = raw_attribute.span.lo.0 as usize - 1;
            let raw_idx_end = raw_idx_start
                + raw_attribute.name.len()
                + raw_attribute.prefix.as_ref().map_or(0, |v| v.len() + 1); // 1 for `:` in e.g. `xmlns:xlink`

            raw_attribute.name = FervidAtom::from(&self.input[raw_idx_start..raw_idx_end]);

            match self.try_parse_directive(raw_attribute, attrs_or_bindings, vue_directives) {
                Ok(()) => {
                    // do nothing, we are good already
                }

                // parse as a raw attribute
                Err(raw_attribute) => {
                    attrs_or_bindings.push(create_regular_attribute(raw_attribute))
                }
            }
        }

        // No `v-pre` found in attributes
        false
    }

    /// Returns `true` if it was recognized as a directive (regardless if it was successfully parsed)
    pub fn try_parse_directive(
        &mut self,
        raw_attribute: Attribute,
        attrs_or_bindings: &mut Vec<AttributeOrBinding>,
        vue_directives: &mut Option<Box<VueDirectives>>,
    ) -> Result<(), Attribute> {
        macro_rules! bail {
            // Everything is okay, this is just not a directive
            () => {
                return Err(raw_attribute);
            };
            // Parsing directive failed
            ($err_kind: expr) => {
                self.errors.push(ParseError {
                    kind: $err_kind,
                    span: raw_attribute.span,
                });
                return Err(raw_attribute);
            };
            // Parsing expression failed
            (js, $parse_error: expr) => {
                self.errors.push($parse_error);
                return Err(raw_attribute);
            };
            ($err_kind: expr, $span: expr) => {
                self.errors.push(ParseError {
                    kind: $err_kind,
                    span: $span,
                });
                return Err(raw_attribute);
            };
        }

        macro_rules! ts {
            () => {
                Syntax::Typescript(Default::default())
            };
        }

        // TODO Should the span be narrower? (It can be narrowed with lo = lo + name.len() + 1 and hi = hi - 1)
        let span = raw_attribute.span;
        let raw_name: &str = &raw_attribute.name;
        let mut chars_iter = raw_name.chars().enumerate();

        enum ParsingMode {
            DirectivePrefix,
            DirectiveName,
            Argument,
            DynamicArgument,
            AfterDynamicArgument,
            Modifier,
        }

        // https://vuejs.org/api/built-in-directives.html#v-bind
        let mut current_start = 0;
        let mut directive_name = "";
        let mut argument_name = "";
        let mut is_argument_dynamic = false;
        let mut is_bind_prop = false;
        let mut current_bracket_level = 0; // for counting `[]` inside dynamic argument
        let mut dynamic_argument_start = 0;
        let mut dynamic_argument_end = 0;
        let mut modifiers = Vec::<FervidAtom>::new();
        let mut parsing_mode = ParsingMode::DirectivePrefix;

        while let Some((idx, c)) = chars_iter.next() {
            match (&parsing_mode, c) {
                // Every directive starts with a prefix: `@`, `:`, `.`, `#` or `v-`
                (ParsingMode::DirectivePrefix, '@') => {
                    directive_name = "on";
                    parsing_mode = ParsingMode::Argument;
                }
                (ParsingMode::DirectivePrefix, ':') => {
                    directive_name = "bind";
                    parsing_mode = ParsingMode::Argument;
                }
                (ParsingMode::DirectivePrefix, '.') => {
                    directive_name = "bind";
                    is_bind_prop = true;
                    parsing_mode = ParsingMode::Argument;
                }
                (ParsingMode::DirectivePrefix, '#') => {
                    directive_name = "slot";
                    parsing_mode = ParsingMode::Argument;
                }
                (ParsingMode::DirectivePrefix, 'v')
                    if matches!(chars_iter.next(), Some((_, '-'))) =>
                {
                    parsing_mode = ParsingMode::DirectiveName;
                }
                (ParsingMode::DirectivePrefix, _) => {
                    // Not a directive
                    bail!();
                }

                (ParsingMode::DirectiveName, c) => {
                    if c == '.' || c == ':' {
                        if current_start == 0 {
                            bail!(ParseErrorKind::DirectiveSyntaxDirectiveName);
                        }

                        directive_name = &raw_name[current_start..idx];
                        current_start = 0;

                        if c == '.' {
                            parsing_mode = ParsingMode::Modifier;
                        } else {
                            parsing_mode = ParsingMode::Argument;
                        }

                        continue;
                    }

                    if current_start == 0 {
                        current_start = idx;
                    }
                }

                (ParsingMode::Argument, '.') => {
                    if current_start == 0 {
                        bail!(ParseErrorKind::DirectiveSyntaxArgument);
                    }

                    argument_name = &raw_name[current_start..idx];
                    current_start = 0;
                    parsing_mode = ParsingMode::Modifier;
                }
                (ParsingMode::Argument, '[') => {
                    if current_start == 0 {
                        parsing_mode = ParsingMode::DynamicArgument;
                    }
                    // Ignore otherwise - argument will be treated as non-dynamic.
                    // For example, `:foo[bar]` is not dynamic, while `:[foo]` is
                }
                (ParsingMode::Argument, _) => {
                    if current_start == 0 {
                        current_start = idx;
                    }
                }

                (ParsingMode::DynamicArgument, '[') => {
                    current_bracket_level += 1;
                }
                (ParsingMode::DynamicArgument, ']') => {
                    if current_bracket_level == 0 {
                        if current_start == 0 {
                            bail!(ParseErrorKind::DirectiveSyntaxDynamicArgument);
                        }

                        argument_name = &raw_name[current_start..idx];
                        is_argument_dynamic = true;
                        dynamic_argument_end = idx;
                        current_start = 0;
                        parsing_mode = ParsingMode::AfterDynamicArgument;
                    } else {
                        current_bracket_level -= 1;
                    }
                }
                (ParsingMode::DynamicArgument, _) => {
                    if current_start == 0 {
                        current_start = idx;
                        dynamic_argument_start = idx;
                    }
                }

                (ParsingMode::AfterDynamicArgument, '.') => {
                    parsing_mode = ParsingMode::Modifier;
                }
                (ParsingMode::AfterDynamicArgument, _) => {
                    bail!(ParseErrorKind::DirectiveSyntaxUnexpectedCharacterAfterDynamicArgument);
                }

                (ParsingMode::Modifier, '.') => {
                    if current_start == 0 {
                        bail!(ParseErrorKind::DirectiveSyntaxModifier);
                    }

                    let modifier_name = &raw_name[current_start..idx];
                    modifiers.push(FervidAtom::from(modifier_name));
                    current_start = 0;
                }
                (ParsingMode::Modifier, _) => {
                    if current_start == 0 {
                        current_start = idx;
                    }
                }
            }
        }

        // Handle end of argument name
        match parsing_mode {
            ParsingMode::DirectivePrefix => {
                bail!();
            }
            ParsingMode::DirectiveName => {
                if current_start == 0 {
                    bail!(ParseErrorKind::DirectiveSyntaxDirectiveName);
                } else {
                    directive_name = &raw_name[current_start..]
                }
            }
            ParsingMode::Argument => {
                if current_start == 0 {
                    bail!(ParseErrorKind::DirectiveSyntaxArgument);
                } else {
                    argument_name = &raw_name[current_start..];
                }
            }
            ParsingMode::Modifier => {
                if current_start == 0 {
                    bail!(ParseErrorKind::DirectiveSyntaxModifier);
                } else {
                    let modifier = &raw_name[current_start..];
                    modifiers.push(FervidAtom::from(modifier));
                }
            }
            ParsingMode::DynamicArgument => {
                // Doesn't matter if it was started or not - it was not closed
                bail!(ParseErrorKind::DirectiveSyntaxDynamicArgument);
            }
            ParsingMode::AfterDynamicArgument => {
                // this mode means that we just parsed a dynamic argument
                // and expect either start of modifier or end of attribute name
            }
        }

        // Try parsing argument (it is optional and may be empty though)
        let argument = match (argument_name, is_argument_dynamic) {
            ("", _) => None,

            (static_name, false) => {
                let argument_start = static_name.as_ptr() as usize - raw_name.as_ptr() as usize;
                let span_lo = raw_attribute.span.lo.0 + argument_start as u32;
                let span = Span {
                    lo: BytePos(span_lo),
                    hi: BytePos(span_lo + static_name.len() as u32),
                };

                Some(StrOrExpr::Str(Str {
                    span,
                    value: static_name.into(),
                    raw: None,
                }))
            }

            (dynamic_name, true) => {
                let attr_lo = raw_attribute.span.lo.0;
                let span_lo = attr_lo + dynamic_argument_start as u32;
                let span_hi = attr_lo + dynamic_argument_end as u32;
                let span = Span {
                    lo: BytePos(span_lo),
                    hi: BytePos(span_hi),
                };

                let parsed = match self.parse_expr(dynamic_name, ts!(), span) {
                    Ok(parsed) => parsed,
                    Err(expr_err) => {
                        bail!(js, expr_err);
                    }
                };

                Some(StrOrExpr::Expr(parsed))
            }
        };

        /// Unwrapping the value or failing
        macro_rules! expect_value {
            () => {
                if let Some(ref value) = raw_attribute.value {
                    value
                } else {
                    bail!(ParseErrorKind::DirectiveSyntax);
                }
            };
        }

        macro_rules! get_directives {
            () => {
                vue_directives.get_or_insert_with(|| Box::new(VueDirectives::default()))
            };
        }

        macro_rules! push_directive {
            ($key: ident, $value: expr) => {
                let directives = get_directives!();
                directives.$key = Some($value);
            };
        }

        macro_rules! push_directive_js {
            ($key: ident, $value: expr) => {
                match self.parse_expr($value, ts!(), span) {
                    Ok(parsed) => {
                        let directives = get_directives!();
                        directives.$key = Some(parsed);
                    }
                    Result::Err(expr_err) => self.report_error(expr_err),
                }
            };
        }

        // Construct the directives from parts
        match directive_name {
            // Directives arranged by estimated usage frequency
            "bind" => {
                // Get flags
                let mut is_camel = false;
                let mut is_prop = is_bind_prop;
                let mut is_attr = false;
                for modifier in modifiers.iter() {
                    match modifier.as_ref() {
                        "camel" => is_camel = true,
                        "prop" => is_prop = true,
                        "attr" => is_attr = true,
                        _ => {}
                    }
                }

                let value = match raw_attribute.value {
                    Some(ref value) => Cow::Borrowed(value.as_str()),
                    None => {
                        // v-bind without a value is a shorthand (e.g. just `:foo-bar` is `:foo-bar="fooBar"`).
                        // This only works for static arguments
                        if let Some(StrOrExpr::Str(ref s)) = argument {
                            let mut out = String::with_capacity(raw_name.len());
                            to_camel_case(&s.value, &mut out);
                            Cow::Owned(out)
                        } else {
                            bail!(ParseErrorKind::DirectiveSyntax);
                        }
                    }
                };

                let parsed_expr = match self.parse_expr(&value, ts!(), span) {
                    Ok(parsed) => parsed,
                    Err(expr_err) => {
                        bail!(js, expr_err);
                    }
                };

                attrs_or_bindings.push(AttributeOrBinding::VBind(VBindDirective {
                    argument,
                    value: parsed_expr,
                    is_camel,
                    is_prop,
                    is_attr,
                    span,
                }));
            }

            "on" => {
                let handler = match raw_attribute.value {
                    Some(ref value) => match self.parse_expr(value, ts!(), span) {
                        Ok(parsed) => Some(parsed),
                        Err(expr_err) => {
                            bail!(js, expr_err);
                        }
                    },
                    None => None,
                };

                attrs_or_bindings.push(AttributeOrBinding::VOn(VOnDirective {
                    event: argument,
                    handler,
                    modifiers,
                    span,
                }));
            }

            "if" => {
                let value = expect_value!();
                push_directive_js!(v_if, &value);
            }

            "else-if" => {
                let value = expect_value!();
                push_directive_js!(v_else_if, &value);
            }

            "else" => {
                push_directive!(v_else, ());
            }

            "for" => {
                let value = expect_value!();

                let Some((source, value, key, index)) = parse_v_for_value(value, span) else {
                    bail!(ParseErrorKind::DirectiveSyntax);
                };

                let source_result = self.parse_expr(source.0, ts!(), source.1);
                let value_result = self.parse_expr(value.0, ts!(), value.1);
                let key_result = key
                    .map(|(raw, span)| self.parse_expr(raw, ts!(), span))
                    .transpose();
                let index_result = index
                    .map(|(raw, span)| self.parse_expr(raw, ts!(), span))
                    .transpose();

                match (source_result, value_result, key_result, index_result) {
                    (Ok(source), Ok(value), Ok(key), Ok(index)) => {
                        push_directive!(
                            v_for,
                            VForDirective {
                                parse_result: Box::new(ForParseResult {
                                    source,
                                    value,
                                    key,
                                    index,
                                    finalized: false,
                                    finalized_is_dynamic: false,
                                }),
                                patch_flags: Default::default(),
                                span,
                            }
                        );
                    }
                    (source, value, key, index) => {
                        if let Err(error) = source {
                            self.report_error(error);
                        }
                        if let Err(error) = value {
                            self.report_error(error);
                        }
                        if let Err(error) = key {
                            self.report_error(error);
                        }
                        if let Err(error) = index {
                            self.report_error(error);
                        }
                    }
                }
            }

            "model" => {
                let value = expect_value!();

                if let Ok(model_binding) = self.parse_expr(value, ts!(), span) {
                    // v-model value must be a valid JavaScript member expression
                    if !matches!(*model_binding, Expr::Member(_) | Expr::Ident(_)) {
                        // TODO Report an error
                        bail!();
                    }

                    let directives = get_directives!();
                    directives.v_model.push(VModelDirective {
                        argument,
                        value: model_binding,
                        update_handler: None,
                        modifiers,
                        span,
                    });
                }
            }

            "slot" => {
                let value =
                    raw_attribute
                        .value
                        .and_then(|v| match self.parse_pat(&v, ts!(), span) {
                            Ok(value) => Some(Box::new(value)),
                            Result::Err(_) => None,
                        });
                push_directive!(
                    v_slot,
                    VSlotDirective {
                        slot_name: argument,
                        value,
                    }
                );
            }

            "show" => {
                let value = expect_value!();
                push_directive_js!(v_show, &value);
            }

            "html" => {
                let value = expect_value!();
                push_directive_js!(v_html, &value);
            }

            "text" => {
                let value = expect_value!();
                push_directive_js!(v_text, &value);
            }

            "once" => {
                push_directive!(v_once, ());
            }

            "pre" => {
                push_directive!(v_pre, ());
            }

            "memo" => {
                let value = expect_value!();
                push_directive_js!(v_memo, &value);
            }

            "cloak" => {
                push_directive!(v_cloak, ());
            }

            // Custom
            _ => 'custom: {
                // If no value, include as is
                let Some(value) = raw_attribute.value else {
                    let directives = get_directives!();
                    directives.custom.push(VCustomDirective {
                        name: directive_name.into(),
                        argument,
                        modifiers,
                        value: None,
                    });
                    break 'custom;
                };

                // If there is a value, try parsing it and only include the successfully parsed values
                match self.parse_expr(&value, ts!(), span) {
                    Ok(parsed) => {
                        let directives = get_directives!();
                        directives.custom.push(VCustomDirective {
                            name: directive_name.into(),
                            argument,
                            modifiers,
                            value: Some(parsed),
                        });
                    }
                    Result::Err(expr_err) => self.report_error(expr_err),
                }
            }
        }

        Ok(())
    }
}

/// Creates `AttributeOrBinding::RegularAttribute`
#[inline]
pub fn create_regular_attribute(raw_attribute: Attribute) -> AttributeOrBinding {
    AttributeOrBinding::RegularAttribute {
        name: raw_attribute.name,
        value: raw_attribute.value.unwrap_or_default(),
        span: raw_attribute.span,
    }
}

type ForExpression<'a> = (&'a str, Span);

fn parse_v_for_value(
    raw: &str,
    original_span: Span,
) -> Option<(
    ForExpression<'_>,
    ForExpression<'_>,
    Option<ForExpression<'_>>,
    Option<ForExpression<'_>>,
)> {
    // `item in iterable` or `item of iterable`
    let split_idx = raw.find(" in ").or_else(|| raw.find(" of "))?;
    const SPLIT_LEN: usize = " in ".len();

    // Get the trimmed source and its span
    let source_start = split_idx + SPLIT_LEN;
    let source = trim_for_expression(raw, source_start, raw.len(), original_span.lo)?;

    // Get the trimmed aliases and their spans
    let mut aliases = &raw[..split_idx];
    let aliases_old_len = aliases.len();
    aliases = aliases.trim_start();
    let mut aliases_start = aliases_old_len - aliases.len();
    aliases = aliases.trim_end();
    let mut aliases_end = aliases_start + aliases.len();

    if raw.as_bytes().get(aliases_start) == Some(&b'(')
        && raw.as_bytes().get(aliases_end - 1) == Some(&b')')
    {
        aliases_start += 1;
        aliases_end -= 1;
    }

    let mut alias_ranges = Vec::with_capacity(3);
    let mut alias_start = aliases_start;
    let mut nesting = 0u32;
    for (relative_index, byte) in raw[aliases_start..aliases_end].bytes().enumerate() {
        let index = aliases_start + relative_index;
        match byte {
            b'(' | b'[' | b'{' => nesting += 1,
            b')' | b']' | b'}' => nesting = nesting.saturating_sub(1),
            b',' if nesting == 0 => {
                alias_ranges.push((alias_start, index));
                alias_start = index + 1;
            }
            _ => {}
        }
    }
    alias_ranges.push((alias_start, aliases_end));

    if alias_ranges.is_empty() || alias_ranges.len() > 3 {
        return None;
    }

    let value = trim_for_expression(raw, alias_ranges[0].0, alias_ranges[0].1, original_span.lo)?;
    let key = alias_ranges
        .get(1)
        .and_then(|&(start, end)| trim_for_expression(raw, start, end, original_span.lo));
    let index = alias_ranges
        .get(2)
        .and_then(|&(start, end)| trim_for_expression(raw, start, end, original_span.lo));

    Some((source, value, key, index))
}

fn trim_for_expression(
    raw: &str,
    start: usize,
    end: usize,
    original_lo: BytePos,
) -> Option<ForExpression<'_>> {
    let mut expression = &raw[start..end];
    let expression_old_len = expression.len();
    expression = expression.trim_start();
    let expression_lo =
        BytePos(original_lo.0 + start as u32 + (expression_old_len - expression.len()) as u32);
    expression = expression.trim_end();
    let expression_hi = BytePos(expression_lo.0 + expression.len() as u32);

    if expression.is_empty() {
        return None;
    }

    Some((
        expression,
        Span {
            lo: expression_lo,
            hi: expression_hi,
        },
    ))
}

/// `foo-bar-baz` -> `fooBarBaz`
#[inline]
fn to_camel_case(raw: &str, out: &mut String) {
    for (idx, word) in raw.split('-').enumerate() {
        if idx == 0 {
            out.push_str(word);
            continue;
        }

        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                out.push(ch_component);
            }

            // Push the rest of the word
            out.push_str(&word[ch.len_utf8()..]);
        }
    }
}

#[cfg(test)]
mod tests {
    use swc_core::ecma::ast::Expr;

    use super::*;

    #[test]
    fn it_parses_regular_attr() {
        test_parse_into_attr("disabled", "true");
        test_parse_into_attr("foo.bar", "true");
        test_parse_into_attr("foo:bar", "true");
        test_parse_into_attr("foo@bar", "true");
        test_parse_into_attr("foo#bar", "true");
        test_parse_into_attr("v.", "true");
    }

    #[test]
    fn it_parses_v_on() {
        assert!(matches!(
            test_parse_into_attr_or_binding("v-on", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: None,
                handler: Some(_),
                modifiers,
                ..
            })) if modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("v-on:click", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Str(s)),
                handler: Some(_),
                modifiers,
                ..
            })) if s.value == "click" && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("@click", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Str(s)),
                handler: Some(_),
                modifiers,
                ..
            })) if s.value == "click" && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("@click.mod1.mod2", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Str(s)),
                handler: Some(_),
                modifiers,
                ..
            })) if s.value == "click" && modifiers.len() == 2
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("@[click]", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Expr(expr)),
                handler: Some(_),
                modifiers,
                ..
            })) if expr.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("@[click.click]", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Expr(expr)),
                handler: Some(_),
                modifiers,
                ..
            })) if expr.is_member() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("@[click[click]]", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Expr(expr)),
                handler: Some(_),
                modifiers,
                ..
            })) if expr.is_member() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("@[click].mod1", "handle"),
            Some(AttributeOrBinding::VOn(VOnDirective {
                event: Some(StrOrExpr::Expr(expr)),
                handler: Some(_),
                modifiers,
                ..
            })) if expr.is_ident() && modifiers.len() == 1
        ));
    }

    #[test]
    fn it_parses_v_bind() {
        assert!(matches!(
            test_parse_into_attr_or_binding("v-bind", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: None,
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding("v-bind:arg-name", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "arg-name"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg-name", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "arg-name"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg.mod1", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "arg"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg.camel", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: true,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "arg"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg.prop", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: true,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "arg"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg.attr", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: true,
                ..
            })) if value.is_ident() && arg.value == "arg"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg.camel.attr.prop.mod", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: true,
                is_prop: true,
                is_attr: true,
                ..
            })) if value.is_ident() && arg.value == "arg"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(".foo", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: true,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "foo"
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":[arg]", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Expr(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.is_ident()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":[arg.name]", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Expr(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.is_member()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":[arg[name]]", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Expr(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.is_member()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":[arg].mod", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Expr(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.is_ident()
        ));
        assert!(matches!(
            test_parse_into_attr_or_binding(":arg[name]", "value"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if value.is_ident() && arg.value == "arg[name]"
        ));
    }

    #[test]
    fn it_supports_shorthand_v_bind() {
        fn test_parse_into_bind(name: &str) -> Option<AttributeOrBinding> {
            let mut errors = Vec::new();
            let mut parser = SfcParser::new("", &mut errors);

            let mut attrs_or_bindings = Vec::new();
            let mut vue_directives = None;
            let result = parser.try_parse_directive(
                Attribute {
                    span: Span {
                        lo: BytePos(1),
                        hi: BytePos(name.len() as u32),
                    },
                    namespace: None,
                    prefix: None,
                    name: FervidAtom::from(name),
                    raw_name: None,
                    value: None,
                    raw_value: None,
                },
                &mut attrs_or_bindings,
                &mut vue_directives,
            );
            assert!(result.is_ok());
            assert!(attrs_or_bindings.len() <= 1);
            assert!(vue_directives.is_none());
            assert!(errors.is_empty());

            attrs_or_bindings.pop()
        }

        assert!(matches!(
            test_parse_into_bind(":msg"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if arg.value == "msg" && value.as_ident().is_some_and(|v| v.sym == "msg")
        ));
        assert!(matches!(
            test_parse_into_bind(":foo-bar"),
            Some(AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(arg)),
                value,
                is_camel: false,
                is_prop: false,
                is_attr: false,
                ..
            })) if arg.value == "foo-bar" && value.as_ident().is_some_and(|v| v.sym == "fooBar")
        ));
    }

    #[test]
    fn it_tracks_static_argument_span() {
        const SOURCE: &str = r#"<div v-bind:argument="value"></div>"#;

        let mut errors = Vec::new();
        let mut parser = SfcParser::new(SOURCE, &mut errors);
        let mut attrs_or_bindings = Vec::new();
        let mut vue_directives = None;

        let result = parser.try_parse_directive(
            Attribute {
                span: Span {
                    lo: BytePos(6),
                    hi: BytePos(29),
                },
                namespace: None,
                prefix: None,
                name: FervidAtom::from("v-bind:argument"),
                raw_name: None,
                value: Some(FervidAtom::from("value")),
                raw_value: None,
            },
            &mut attrs_or_bindings,
            &mut vue_directives,
        );

        assert!(result.is_ok());
        let Some(AttributeOrBinding::VBind(VBindDirective {
            argument: Some(StrOrExpr::Str(argument)),
            ..
        })) = attrs_or_bindings.pop()
        else {
            panic!("Expected v-bind with a static argument")
        };

        assert_eq!(
            argument.span,
            Span {
                lo: BytePos(13),
                hi: BytePos(21),
            }
        );
        assert_eq!(
            &SOURCE[argument.span.lo.0 as usize - 1..argument.span.hi.0 as usize - 1],
            "argument"
        );
    }

    #[test]
    fn it_parses_v_slot() {
        fn test_parse_into_slot(name: &str, value: &str) -> VSlotDirective {
            let directives = test_parse_into_vue_directive(name, value);
            directives.v_slot.expect("Slot directive should exist")
        }

        assert!(matches!(
            test_parse_into_slot("v-slot", ""),
            VSlotDirective {
                slot_name: None,
                value: None
            }
        ));
        assert!(matches!(
            test_parse_into_slot("v-slot", "value"),
            VSlotDirective {
                slot_name: None,
                value: Some(value)
            } if value.is_ident()
        ));
        assert!(matches!(
            test_parse_into_slot("v-slot:default", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Str(name)),
                value: Some(value)
            } if value.is_ident() && name.value == "default"
        ));
        assert!(matches!(
            test_parse_into_slot("v-slot:[slot]", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Expr(name)),
                value: Some(value)
            } if value.is_ident() && name.is_ident()
        ));
        assert!(matches!(
            test_parse_into_slot("v-slot:[slot.name]", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Expr(name)),
                value: Some(value)
            } if value.is_ident() && name.is_member()
        ));
        assert!(matches!(
            test_parse_into_slot("v-slot:[slot[name]]", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Expr(name)),
                value: Some(value)
            } if value.is_ident() && name.is_member()
        ));
        assert!(matches!(
            test_parse_into_slot("#default", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Str(name)),
                value: Some(value)
            } if value.is_ident() && name.value == "default"
        ));
        assert!(matches!(
            test_parse_into_slot("#[slot]", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Expr(name)),
                value: Some(value)
            } if value.is_ident() && name.is_ident()
        ));
        assert!(matches!(
            test_parse_into_slot("#[slot.name]", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Expr(name)),
                value: Some(value)
            } if value.is_ident() && name.is_member()
        ));
        assert!(matches!(
            test_parse_into_slot("#[slot[name]]", "value"),
            VSlotDirective {
                slot_name: Some(StrOrExpr::Expr(name)),
                value: Some(value)
            } if value.is_ident() && name.is_member()
        ));
    }

    #[test]
    fn it_parses_v_for() {
        let directives = test_parse_into_vue_directive("v-for", "(value, key, index) in source");
        let result = directives
            .v_for
            .expect("v-for directive should exist")
            .parse_result;

        assert!(result.source.as_ident().is_some_and(|v| v.sym == "source"));
        assert!(result.value.as_ident().is_some_and(|v| v.sym == "value"));
        assert!(
            result
                .key
                .as_deref()
                .and_then(Expr::as_ident)
                .is_some_and(|v| v.sym == "key")
        );
        assert!(
            result
                .index
                .as_deref()
                .and_then(Expr::as_ident)
                .is_some_and(|v| v.sym == "index")
        );
        assert!(!result.finalized);

        let directives = test_parse_into_vue_directive("v-for", "([a, b], i) of items");
        let result = directives
            .v_for
            .expect("v-for directive should exist")
            .parse_result;
        assert!(result.value.is_array());
        assert!(result.key.as_deref().is_some_and(Expr::is_ident));
        assert!(result.index.is_none());
    }

    #[test]
    fn it_parses_custom_directive() {
        fn test_parse_into_custom(name: &str, value: &str) -> VCustomDirective {
            let mut directives = test_parse_into_vue_directive(name, value);
            directives
                .custom
                .pop()
                .expect("Custom directive should exist")
        }

        assert!(matches!(
            test_parse_into_custom("v-custom-dir", "value"),
            VCustomDirective {
                name,
                argument: None,
                modifiers,
                value: Some(v)
            } if name == "custom-dir" && v.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:arg-name", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Str(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.value == "arg-name" && v.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:[arg-name]", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Expr(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.is_bin() && v.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:[arg.name]", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Expr(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.is_member() && v.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:[arg[name]]", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Expr(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.is_member() && v.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:arg[name]", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Str(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.value == "arg[name]" && v.is_ident() && modifiers.is_empty()
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom.mod1.mod2", "value"),
            VCustomDirective {
                name,
                argument: None,
                modifiers,
                value: Some(v)
            } if name == "custom" && v.is_ident() && modifiers.len() == 2
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom.[mod1.mod2]", "value"),
            VCustomDirective {
                name,
                argument: None,
                modifiers,
                value: Some(v)
            } if name == "custom" && v.is_ident() && modifiers.len() == 2
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:arg.mod1", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Str(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.value == "arg" && v.is_ident() && modifiers.len() == 1
        ));
        assert!(matches!(
            test_parse_into_custom("v-custom:[arg].mod1", "value"),
            VCustomDirective {
                name,
                argument: Some(StrOrExpr::Expr(arg)),
                modifiers,
                value: Some(v)
            } if name == "custom" && arg.is_ident() && v.is_ident() && modifiers.len() == 1
        ));
    }

    #[test]
    fn it_parses_v_for_value() {
        macro_rules! check {
            ($input:expr, $source:expr, $value:expr, $key:expr, $index:expr) => {
                let input = $input;
                let span = Span {
                    lo: BytePos(1),
                    hi: BytePos((input.len() + 1) as u32),
                };

                let (source, value, key, index) =
                    parse_v_for_value(input, span).expect("Did not parse v-for value");
                assert_eq!($source, source.0);
                assert_eq!($value, value.0);
                assert_eq!($key, key.map(|v| v.0));
                assert_eq!($index, index.map(|v| v.0));

                for (raw, expression_span) in [Some(source), Some(value), key, index]
                    .into_iter()
                    .flatten()
                {
                    let start = (expression_span.lo.0 - span.lo.0) as usize;
                    let end = (expression_span.hi.0 - span.lo.0) as usize;
                    assert_eq!(raw, &input[start..end]);
                }
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

    fn test_parse_into_attr(name: &str, value: &str) {
        let mut errors = Vec::new();
        let mut parser = SfcParser::new("", &mut errors);

        let mut attrs_or_bindings = Vec::new();
        let mut vue_directives = None;
        let result = parser.try_parse_directive(
            Attribute {
                span: Span {
                    lo: BytePos(1),
                    hi: BytePos((name.len() + value.len() + 3) as u32),
                },
                namespace: None,
                prefix: None,
                name: FervidAtom::from(name),
                raw_name: None,
                value: Some(FervidAtom::from(value)),
                raw_value: None,
            },
            &mut attrs_or_bindings,
            &mut vue_directives,
        );
        assert!(result.is_err());
    }

    fn test_parse_into_attr_or_binding(name: &str, value: &str) -> Option<AttributeOrBinding> {
        let mut errors = Vec::new();
        let mut parser = SfcParser::new("", &mut errors);

        let mut attrs_or_bindings = Vec::new();
        let mut vue_directives = None;
        let result = parser.try_parse_directive(
            Attribute {
                span: Span {
                    lo: BytePos(1),
                    hi: BytePos((name.len() + value.len() + 3) as u32),
                },
                namespace: None,
                prefix: None,
                name: FervidAtom::from(name),
                raw_name: None,
                value: Some(FervidAtom::from(value)),
                raw_value: None,
            },
            &mut attrs_or_bindings,
            &mut vue_directives,
        );
        assert!(result.is_ok());
        assert!(attrs_or_bindings.len() <= 1);
        assert!(vue_directives.is_none());
        assert!(errors.is_empty());

        attrs_or_bindings.pop()
    }

    fn test_parse_into_vue_directive(name: &str, value: &str) -> Box<VueDirectives> {
        let mut errors = Vec::new();
        let mut parser = SfcParser::new("", &mut errors);

        let mut attrs_or_bindings = Vec::new();
        let mut vue_directives = None;
        let result = parser.try_parse_directive(
            Attribute {
                span: Span {
                    lo: BytePos(1),
                    hi: BytePos((name.len() + value.len() + 3) as u32),
                },
                namespace: None,
                prefix: None,
                name: FervidAtom::from(name),
                raw_name: None,
                value: Some(FervidAtom::from(value)),
                raw_value: None,
            },
            &mut attrs_or_bindings,
            &mut vue_directives,
        );
        assert!(result.is_ok());
        assert!(attrs_or_bindings.is_empty());
        assert!(errors.is_empty());

        vue_directives.expect("Directives should exist")
    }
}
