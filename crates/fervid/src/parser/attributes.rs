use fervid_core::{
    AttributeOrBinding, StrOrExpr, VBindDirective, VCustomDirective, VForDirective,
    VModelDirective, VOnDirective, VSlotDirective, VueDirectives,
};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till},
    character::complete::char,
    combinator::fail,
    error::{ErrorKind, ParseError},
    multi::many0,
    sequence::{delimited, preceded},
    Err, IResult,
};
use swc_core::common::DUMMY_SP;

use crate::parser::{
    ecma::{parse_js, parse_js_pat},
    html_utils::html_name,
};

pub fn parse_attributes(
    input: &str,
) -> IResult<&str, (Vec<AttributeOrBinding>, Option<Box<VueDirectives>>)> {
    let mut directives = None;
    let mut attrs = Vec::new();
    let mut input = input;

    loop {
        let len = input.len();

        // Skip whitespace
        input = input.trim_start();

        // Try parsing a directive first
        let directive_result = parse_directive(input, &mut attrs, &mut directives);
        match directive_result {
            // Err(Err::Error(_)) => return Ok((input, (attrs, directives))),
            // Err(e) => return Err(e),
            Ok((new_input, _)) => {
                // infinite loop check: the parser must always consume
                if new_input.len() == len {
                    return Err(Err::Error(ParseError::from_error_kind(
                        input,
                        ErrorKind::Many0,
                    )));
                }

                input = new_input;
            }

            Err(_) => {
                // Try parsing a regular attribute
                let attribute_result = parse_vanilla_attr(input, &mut attrs);

                match attribute_result {
                    Err(Err::Error(_)) => return Ok((input, (attrs, directives))),
                    Err(e) => return Err(e),
                    Ok((new_input, _)) => {
                        // infinite loop check: the parser must always consume
                        if new_input.len() == len {
                            return Err(Err::Error(ParseError::from_error_kind(
                                input,
                                ErrorKind::Many0,
                            )));
                        }

                        input = new_input;
                    }
                }
            }
        }
    }
}

fn parse_vanilla_attr<'i>(
    input: &'i str,
    out: &mut Vec<AttributeOrBinding<'i>>,
) -> IResult<&'i str, ()> {
    let (input, attr_name) = html_name(input)?;

    /* Support omitting a `=` char */
    let eq: Result<(&str, char), nom::Err<nom::error::Error<_>>> = char('=')(input);
    match eq {
        // consider omitted attribute as attribute name itself (as current Vue compiler does)
        Err(_) => {
            out.push(AttributeOrBinding::RegularAttribute {
                name: attr_name,
                value: &attr_name,
            });
            Ok((input, ()))
        }

        Ok((input, _)) => {
            let (input, attr_value) = parse_attr_value(input)?;

            #[cfg(dbg_print)]
            println!("Dynamic attr: value = {:?}", attr_value);

            out.push(AttributeOrBinding::RegularAttribute {
                name: attr_name,
                value: attr_value,
            });

            Ok((input, ()))
        }
    }
}

fn parse_attr_value(input: &str) -> IResult<&str, &str> {
    delimited(char('"'), take_till(|c| c == '"'), char('"'))(input)
}

/// Parses a directive in form of `v-directive-name:directive-attribute.modifier1.modifier2`
///
/// Allows for shortcuts like `@` (same as `v-on`), `:` (`v-bind`) and `#` (`v-slot`)
fn parse_directive<'i>(
    input: &'i str,
    attributes: &mut Vec<AttributeOrBinding<'i>>,
    directives: &mut Option<Box<VueDirectives<'i>>>,
) -> IResult<&'i str, ()> {
    let (input, prefix) = alt((tag("v-"), tag("@"), tag("#"), tag(":"), tag(".")))(input)?;

    // https://vuejs.org/api/built-in-directives.html#v-bind
    let mut is_bind_prop = false;
    let mut is_dynamic = false;

    // Determine directive name
    let mut has_argument = false;
    let (input, directive_name) = match prefix {
        "v-" => {
            let (input, name) = html_name(input)?;

            // next char is colon, shift input and set flag
            if let Some(':') = input.chars().next() {
                has_argument = true;
                (&input[1..], name)
            } else {
                (input, name)
            }
        }

        "@" => {
            has_argument = true;
            (input, "on")
        }

        ":" => {
            has_argument = true;
            (input, "bind")
        }

        "." => {
            has_argument = true;
            is_bind_prop = true;
            (input, "bind")
        }

        "#" => {
            has_argument = true;
            (input, "slot")
        }

        _ => {
            return Err(nom::Err::Error(nom::error::Error {
                code: nom::error::ErrorKind::Tag,
                input,
            }))
        }
    };

    // Read argument part if we spotted `:` earlier
    let (input, argument) = if has_argument {
        // Support v-slot:[slotname], v-bind:[attr], etc.
        let (input, arg) = if input.starts_with("[") {
            is_dynamic = true;

            delimited(char('['), html_name, char(']'))(input)?
        } else {
            html_name(input)?
        };

        (input, Some(arg))
    } else {
        (input, None)
    };

    #[cfg(dbg_print)]
    {
        println!();
        println!("Parsed directive {:?}", directive_name);
        println!("Has argument: {}, argument: {:?}", has_argument, argument);
    }

    // Read modifiers
    let (input, modifiers): (&str, Vec<&str>) =
        many0(preceded(char('.'), html_name))(input).unwrap_or((input, vec![]));

    // Value
    let (input, value) = if !input.starts_with('=') {
        (input, None)
    } else {
        let (input, value) = parse_attr_value(&input[1..])?;
        (input, Some(value))
    };

    macro_rules! fail {
        () => {
            // TODO: this fails at a very unexpected location,
            // but maybe it needs to rewind back to the start
            return fail(input);
        };
    }

    /// Unwrapping the value or failing
    macro_rules! expect_value {
        () => {
            if let Some(value) = value {
                value
            } else {
                fail!();
            }
        };
    }

    macro_rules! get_directives {
        () => {
            directives.get_or_insert_with(|| Box::new(VueDirectives::default()))
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
            // TODO span
            match parse_js($value, 0, 0) {
                Ok(parsed) => {
                    let directives = get_directives!();
                    directives.$key = Some(parsed);
                }
                Result::Err(_) => {}
            }
        };
    }

    // Type the directive
    match directive_name {
        // Directives arranged by estimated usage frequency
        "bind" => {
            // Get flags
            let mut is_camel = false;
            let mut is_prop = is_bind_prop;
            let mut is_attr = false;
            for modifier in modifiers.iter() {
                match *modifier {
                    "camel" => is_camel = true,
                    "prop" => is_prop = true,
                    "attr" => is_attr = true,
                    _ => {}
                }
            }

            let value = expect_value!();

            // TODO span
            let Ok(parsed_expr) = parse_js(value, 0, 0) else {
                fail!();
            };

            // TODO don't fail the directive but skip it instead
            let argument = convert_argument(argument, is_dynamic, input)?;

            attributes.push(AttributeOrBinding::VBind(VBindDirective {
                argument,
                value: parsed_expr,
                is_camel,
                is_prop,
                is_attr,
            }));
        }
        "on" => {
            let argument = convert_argument(argument, is_dynamic, input)?;

            attributes.push(AttributeOrBinding::VOn(VOnDirective {
                event: argument,
                handler: value.and_then(|value| {
                    // TODO span
                    let parse_result = parse_js(value, 0, 0);
                    match parse_result {
                        Ok(parsed_expr) => Some(parsed_expr),
                        Err(_) => None,
                    }
                }),
                modifiers,
            }));
        }
        "if" => {
            let value = expect_value!();

            // TODO Span
            match parse_js(value, 0, 0) {
                Ok(condition) => {
                    push_directive!(v_if, condition);
                }
                Result::Err(_) => {}
            }
        }
        "else-if" => {
            let value = expect_value!();

            // TODO Span
            match parse_js(value, 0, 0) {
                Ok(condition) => {
                    push_directive!(v_else_if, condition);
                }
                Result::Err(_) => {}
            }
        }
        "else" => {
            push_directive!(v_else, ());
        }
        "for" => {
            let value = expect_value!();

            let Some((itervar, iterable)) = split_itervar_and_iterable(value) else {
                fail!();
            };

            // TODO Span
            match parse_js(itervar, 0, 0) {
                Ok(itervar) => match parse_js(iterable, 0, 0) {
                    Ok(iterable) => {
                        push_directive!(
                            v_for,
                            VForDirective {
                                iterable,
                                itervar
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

            // TODO Span
            match parse_js(value, 0, 0) {
                Ok(model_binding) => {
                    let directives = get_directives!();
                    directives.v_model.push(VModelDirective {
                        argument,
                        value: *model_binding,
                        modifiers,
                        span: DUMMY_SP, // TODO
                    });
                }
                Result::Err(_) => {}
            }
        }
        "slot" => {
            let value = value.and_then(|v| {
                // TODO Span
                match parse_js_pat(v, 0, 0) {
                    Ok(value) => Some(Box::new(value)),
                    Result::Err(_) => None,
                }
            });
            push_directive!(
                v_slot,
                VSlotDirective {
                    slot_name: argument,
                    value,
                    is_dynamic_slot: is_dynamic
                }
            );
        }
        "show" => {
            let value = expect_value!();
            push_directive_js!(v_show, value);
        }
        "html" => {
            let value = expect_value!();
            push_directive_js!(v_html, value);
        }
        "text" => {
            let value = expect_value!();
            push_directive_js!(v_text, value);
        }
        "once" => {
            push_directive!(v_once, ());
        }
        "pre" => {
            push_directive!(v_pre, ());
        }
        "memo" => {
            let value = expect_value!();
            push_directive_js!(v_memo, value);
        }
        "cloak" => {
            push_directive!(v_cloak, ());
        }

        // Custom
        _ => 'custom: {
            // If no value, include as is
            let Some(value) = value else {
                let directives = get_directives!();
                directives.custom.push(VCustomDirective {
                    name: directive_name,
                    argument,
                    modifiers,
                    value: None,
                });
                break 'custom;
            };

            // If there is a value, try parsing it and only include the successfully parsed values
            match parse_js(value, 0, 0) {
                Ok(parsed) => {
                    let directives = get_directives!();
                    directives.custom.push(VCustomDirective {
                        name: directive_name,
                        argument,
                        modifiers,
                        value: Some(parsed),
                    });
                }
                Result::Err(_) => {}
            }
        }
    };

    Ok((input, ()))
}

/// Converts a raw Option<&str> argument to an argument
/// which value is either a string or a js expression.
/// If parsing Js fails, returns Err.
fn convert_argument<'s>(
    argument: Option<&'s str>,
    is_dynamic: bool,
    input: &'s str
) -> Result<Option<StrOrExpr<'s>>, nom::Err<nom::error::Error<&'s str>>> {
    match argument {
        Some(raw_arg) => {
            if is_dynamic {
                // TODO Span & better error
                let Ok(dynamic_argument) = parse_js(raw_arg, 0, 0) else {
                    return Err(nom::Err::Error(nom::error::Error::from_error_kind(input, ErrorKind::Fail)));
                };

                Ok(Some(StrOrExpr::Expr(dynamic_argument)))
            } else {
                Ok(Some(StrOrExpr::Str(raw_arg)))
            }
        }
        None => Ok(None),
    }
}

// fn parse_dynamic_attr(input: &str) -> IResult<&str, HtmlAttribute> {
//     let (input, directive) = parse_directive(input)?;

//     #[cfg(dbg_print)]
//     println!("Dynamic attr: directive = {:?}", directive);

//     /* Try taking a `=` char, early return if it's not there */
//     if !input.starts_with('=') {
//         return Ok((input, directive));
//     }

//     let (input, attr_value) = parse_attr_value(&input[1..])?;

//     #[cfg(dbg_print)]
//     println!("Dynamic attr: value = {:?}", attr_value);

//     match directive {
//         HtmlAttribute::VDirective(directive) => Ok((
//             input,
//             HtmlAttribute::VDirective(VDirective {
//                 value: Some(attr_value),
//                 ..directive
//             }),
//         )),

//         /* Not possible, because parse_directive returns a directive indeed */
//         _ => Err(nom::Err::Error(nom::error::Error {
//             code: nom::error::ErrorKind::Fail,
//             input,
//         })),
//     }
// }

fn split_itervar_and_iterable<'a>(raw: &'a str) -> Option<(&'a str, &'a str)> {
    // Try guessing: `item in iterable`
    let mut split = raw.splitn(2, " in ");
    if let (Some(itervar), Some(iterable)) = (split.next(), split.next()) {
        return Some((itervar.trim(), iterable.trim()));
    }

    // Try `item of iterable`
    let mut split = raw.splitn(2, " of ");
    if let (Some(itervar), Some(iterable)) = (split.next(), split.next()) {
        return Some((itervar.trim(), iterable.trim()));
    }

    // Not valid
    None
}
