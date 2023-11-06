use fervid_core::{
    AttributeOrBinding, FervidAtom, StrOrExpr, VBindDirective, VCustomDirective, VForDirective,
    VModelDirective, VOnDirective, VSlotDirective, VueDirectives, fervid_atom,
};
use swc_core::common::{BytePos, Span};
use swc_ecma_parser::Syntax;
use swc_html_ast::Attribute;

use crate::{
    error::{ParseError, ParseErrorKind},
    script::{parse_expr, parse_pat},
};

pub fn process_element_attributes(
    raw_attributes: Vec<Attribute>,
    attrs_or_bindings: &mut Vec<AttributeOrBinding>,
    vue_directives: &mut Option<Box<VueDirectives>>,
    errors: &mut Vec<ParseError>,
) {
    for raw_attr in raw_attributes.into_iter() {
        parse_raw_attribute(raw_attr, attrs_or_bindings, vue_directives, errors);
    }
}

pub fn parse_raw_attribute(
    raw_attribute: Attribute,
    attrs_or_bindings: &mut Vec<AttributeOrBinding>,
    vue_directives: &mut Option<Box<VueDirectives>>,
    errors: &mut Vec<ParseError>,
) {
    match try_parse_directive(raw_attribute, attrs_or_bindings, vue_directives, errors) {
        Ok(()) => {
            // do nothing, we are good already
        }
        Err(raw_attribute) => {
            // parse as a raw attribute
            attrs_or_bindings.push(AttributeOrBinding::RegularAttribute {
                name: raw_attribute.name,
                value: raw_attribute.value.unwrap_or_else(|| fervid_atom!("")),
            })
        }
    }
}

/// Returns `true` if it was recognized as a directive (regardless if it was successfully parsed)
pub fn try_parse_directive(
    raw_attribute: Attribute,
    attrs_or_bindings: &mut Vec<AttributeOrBinding>,
    vue_directives: &mut Option<Box<VueDirectives>>,
    errors: &mut Vec<ParseError>,
) -> Result<(), Attribute> {
    macro_rules! bail {
        // Everything is okay, this is just not a directive
        () => {
            return Err(raw_attribute);
        };
        // Parsing directive failed
        ($err_kind: expr) => {
            errors.push(ParseError {
                kind: $err_kind,
                span: raw_attribute.span,
            });
            return Err(raw_attribute);
        };
        (js, $parse_error: expr) => {
            errors.push($parse_error);
            return Err(raw_attribute);
        };
    }

    macro_rules! ts {
        () => {
            Syntax::Typescript(Default::default())
        };
    }

    // TODO Fix and test parsing of directives

    // TODO Should the span be narrower? (It can be narrowed with lo = lo + name.len() + 1 and hi = hi - 1)
    let span = raw_attribute.span;
    let raw_name: &str = &raw_attribute.name;
    let mut chars_iter = raw_name.chars().enumerate().peekable();

    // Every directive starts with a prefix: `@`, `:`, `.`, `#` or `v-`
    let Some((_, prefix)) = chars_iter.next() else {
        bail!(ParseErrorKind::DirectiveSyntax);
    };

    // https://vuejs.org/api/built-in-directives.html#v-bind
    let mut is_bind_prop = false;
    let mut expect_argument = true;
    let mut argument_start = 0;
    let mut argument_end = raw_name.len();

    let directive_name = match prefix {
        '@' => "on",
        ':' => "bind",
        '.' => {
            is_bind_prop = true;
            "bind"
        }
        '#' => "slot",
        'v' if matches!(chars_iter.next(), Some((_, '-'))) => {
            // Read directive name
            let mut start = 0;
            let mut end = raw_name.len();
            while let Some((idx, c)) = chars_iter.next() {
                if c == '.' {
                    expect_argument = false;
                    argument_end = idx;
                    end = idx;
                    break;
                }
                if c == ':' {
                    end = idx;
                    break;
                }
                if start == 0 {
                    // `idx` is never 0 because zero-th char is `prefix`
                    start = idx;
                }
            }

            // Directive syntax is bad if we could not read the directive name
            if start == 0 {
                bail!(ParseErrorKind::DirectiveSyntax);
            }

            &raw_name[start..end]
        }
        _ => {
            bail!();
        }
    };

    // Try parsing argument (it is optional and may be empty though)
    let mut argument: Option<StrOrExpr> = None;
    if expect_argument {
        while let Some((idx, c)) = chars_iter.next() {
            if c == '.' {
                argument_end = idx;
                break;
            }
            if argument_start == 0 {
                argument_start = idx;
            }
        }

        if argument_start != 0 {
            let mut raw_argument = &raw_name[argument_start..argument_end];
            let mut is_dynamic_argument = false;

            // Dynamic argument: `:[dynamic-argument]`
            if raw_argument.starts_with('[') {
                // Check syntax
                if !raw_argument.ends_with(']') {
                    bail!(ParseErrorKind::DynamicArgument);
                }

                raw_argument = &raw_argument['['.len_utf8()..(raw_argument.len() - ']'.len_utf8())];
                if raw_argument.is_empty() {
                    bail!(ParseErrorKind::DynamicArgument);
                }

                is_dynamic_argument = true;
            }

            if is_dynamic_argument {
                // TODO Narrower span?
                let parsed_argument = match parse_expr(raw_argument, ts!(), span) {
                    Ok(parsed) => parsed,
                    Err(expr_err) => {
                        bail!(js, expr_err.into());
                    }
                };

                argument = Some(StrOrExpr::Expr(parsed_argument));
            } else {
                argument = Some(StrOrExpr::Str(FervidAtom::from(raw_argument)));
            }
        }
    }

    // Try parsing modifiers, it is a simple string split
    let mut modifiers = Vec::<FervidAtom>::new();
    if argument_end != 0 {
        for modifier in raw_name[argument_end..]
            .split('.')
            .filter(|m| !m.is_empty())
        {
            modifiers.push(FervidAtom::from(modifier));
        }
    }

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
            match parse_expr($value, ts!(), span) {
                Ok(parsed) => {
                    let directives = get_directives!();
                    directives.$key = Some(parsed);
                }
                Result::Err(_) => {}
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

            let value = expect_value!();

            let parsed_expr = match parse_expr(&value, ts!(), span) {
                Ok(parsed) => parsed,
                Err(expr_err) => {
                    bail!(js, expr_err.into());
                }
            };

            attrs_or_bindings.push(AttributeOrBinding::VBind(VBindDirective {
                argument,
                value: parsed_expr,
                is_camel,
                is_prop,
                is_attr,
            }));
        }

        "on" => {
            let handler = match raw_attribute.value {
                Some(ref value) => match parse_expr(&value, ts!(), span) {
                    Ok(parsed) => Some(parsed),
                    Err(expr_err) => {
                        bail!(js, expr_err.into());
                    }
                },
                None => None,
            };

            attrs_or_bindings.push(AttributeOrBinding::VOn(VOnDirective {
                event: argument,
                handler,
                modifiers,
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

            let Some(((itervar, itervar_span), (iterable, iterable_span))) =
                split_itervar_and_iterable(&value, span)
            else {
                bail!(ParseErrorKind::DirectiveSyntax);
            };

            match parse_expr(itervar, ts!(), itervar_span) {
                Ok(itervar) => match parse_expr(iterable, ts!(), iterable_span) {
                    Ok(iterable) => {
                        push_directive!(
                            v_for,
                            VForDirective {
                                iterable,
                                itervar,
                                patch_flags: Default::default()
                            }
                        );
                    }
                    Result::Err(_) => {}
                },
                Result::Err(_) => {}
            };
        }

        "model" => {
            let value = expect_value!();

            match parse_expr(&value, ts!(), span) {
                Ok(model_binding) => {
                    let directives = get_directives!();
                    directives.v_model.push(VModelDirective {
                        argument,
                        value: *model_binding,
                        modifiers,
                        span,
                    });
                }
                Result::Err(_) => {}
            }
        }

        "slot" => {
            let value = raw_attribute
                .value
                .and_then(|v| match parse_pat(&v, ts!(), span) {
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
            match parse_expr(&value, ts!(), span) {
                Ok(parsed) => {
                    let directives = get_directives!();
                    directives.custom.push(VCustomDirective {
                        name: directive_name.into(),
                        argument,
                        modifiers,
                        value: Some(parsed),
                    });
                }
                Result::Err(_) => {}
            }
        }
    }

    Ok(())
}

fn split_itervar_and_iterable<'a>(
    raw: &'a str,
    original_span: Span,
) -> Option<((&'a str, Span), (&'a str, Span))> {
    // `item in iterable` or `item of iterable`
    let Some(split_idx) = raw.find(" in ").or_else(|| raw.find(" of ")) else {
        return None;
    };
    const SPLIT_LEN: usize = " in ".len();

    // Get the trimmed itervar and its span
    let mut offset = original_span.lo.0;
    let mut itervar = &raw[..split_idx];
    let mut itervar_old_len = itervar.len();
    itervar = itervar.trim_start();
    let itervar_lo = BytePos(offset + (itervar_old_len - itervar.len()) as u32);
    itervar_old_len = itervar.len();
    itervar = itervar.trim_end();
    let itervar_hi = BytePos(offset + (split_idx - (itervar_old_len - itervar.len())) as u32);

    let iterable_start = split_idx + SPLIT_LEN;
    offset += iterable_start as u32;

    let mut iterable = &raw[iterable_start..];
    let iterable_old_len = iterable.len();
    iterable = iterable.trim_start();
    let iterable_lo = BytePos(offset + (iterable_old_len - iterable.len()) as u32);
    iterable = iterable.trim_end();
    let iterable_hi = BytePos(iterable_lo.0 + iterable.len() as u32);

    if itervar.is_empty() || iterable.is_empty() {
        return None;
    }

    let new_span_itervar = Span {
        lo: itervar_lo,
        hi: itervar_hi,
        ctxt: original_span.ctxt,
    };

    let new_span_iterable = Span {
        lo: iterable_lo,
        hi: iterable_hi,
        ctxt: original_span.ctxt,
    };

    Some(((itervar, new_span_itervar), (iterable, new_span_iterable)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_correctly_splits_itervar_iterable() {
        macro_rules! check {
            ($input: expr, $itervar: expr, $itervar_lo: expr, $itervar_hi: expr, $iterable: expr, $iterable_lo: expr, $iterable_hi: expr) => {
                let input = $input;
                let span = Span {
                    lo: BytePos(1),
                    hi: BytePos((input.len() + 1) as u32),
                    ctxt: Default::default(),
                };

                let Some(((itervar, itervar_span), (iterable, iterable_span))) =
                    split_itervar_and_iterable(input, span)
                else {
                    panic!("Did not manage to split")
                };
                assert_eq!($itervar, itervar);
                assert_eq!($itervar_lo, itervar_span.lo.0);
                assert_eq!($itervar_hi, itervar_span.hi.0);
                assert_eq!($iterable, iterable);
                assert_eq!($iterable_lo, iterable_span.lo.0);
                assert_eq!($iterable_hi, iterable_span.hi.0);
            };
        }

        // Trivial (all `Span`s start from 1)
        check!("item in list", "item", 1, 5, "list", 9, 13);
        check!("item of list", "item", 1, 5, "list", 9, 13);
        check!("i in 3", "i", 1, 2, "3", 6, 7);

        // A bit harder
        check!("   item   in \n \t  list   ", "item", 4, 8, "list", 19, 23);
    }
}
