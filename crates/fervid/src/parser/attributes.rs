use fervid_core::{
    HtmlAttribute, VBindDirective, VCustomDirective, VDirective, VForDirective, VModelDirective,
    VOnDirective, VSlotDirective,
};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till},
    character::complete::char,
    combinator::fail,
    multi::many0,
    sequence::{delimited, preceded},
    IResult,
};

use crate::parser::html_utils::{html_name, space1};

fn parse_attr_value(input: &str) -> IResult<&str, &str> {
    delimited(char('"'), take_till(|c| c == '"'), char('"'))(input)
}

/// Parses a directive in form of `v-directive-name:directive-attribute.modifier1.modifier2`
///
/// Allows for shortcuts like `@` (same as `v-on`), `:` (`v-bind`) and `#` (`v-slot`)
fn parse_directive(input: &str) -> IResult<&str, HtmlAttribute> {
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

    // Type the directive
    let directive = match directive_name {
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

            VDirective::Bind(VBindDirective {
                argument,
                value,
                is_dynamic_attr: is_dynamic,
                is_camel,
                is_prop,
                is_attr,
            })
        }
        "on" => VDirective::On(VOnDirective {
            event: argument,
            handler: value,
            is_dynamic_event: is_dynamic,
            modifiers,
        }),
        "if" => {
            let value = expect_value!();
            VDirective::If(value)
        }
        "else-if" => {
            let value = expect_value!();
            VDirective::ElseIf(value)
        }
        "else" => VDirective::Else,
        "for" => {
            let value = expect_value!();

            let Some((iterator, iterable)) = split_itervar_and_iterable(value) else {
                fail!();
            };

            VDirective::For(VForDirective { iterable, iterator })
        }
        "model" => {
            let value = expect_value!();

            VDirective::Model(VModelDirective {
                argument,
                value,
                modifiers,
            })
        }
        "slot" => VDirective::Slot(VSlotDirective {
            slot_name: argument,
            value,
            is_dynamic_slot: is_dynamic
        }),
        "show" => {
            let value = expect_value!();
            VDirective::Show(value)
        }
        "html" => {
            let value = expect_value!();
            VDirective::Html(value)
        }
        "text" => {
            let value = expect_value!();
            VDirective::Text(value)
        }
        "once" => VDirective::Once,
        "pre" => VDirective::Pre,
        "memo" => {
            let value = expect_value!();
            VDirective::Memo(value)
        }
        "cloak" => VDirective::Cloak,

        // Custom
        _ => VDirective::Custom(VCustomDirective {
            name: directive_name,
            argument,
            modifiers,
            value,
        }),
    };

    Ok((input, HtmlAttribute::VDirective(directive)))
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

fn parse_vanilla_attr(input: &str) -> IResult<&str, HtmlAttribute> {
    let (input, attr_name) = html_name(input)?;

    /* Support omitting a `=` char */
    let eq: Result<(&str, char), nom::Err<nom::error::Error<_>>> = char('=')(input);
    match eq {
        // consider omitted attribute as attribute name itself (as current Vue compiler does)
        Err(_) => Ok((
            input,
            HtmlAttribute::Regular {
                name: attr_name,
                value: &attr_name,
            },
        )),

        Ok((input, _)) => {
            let (input, attr_value) = parse_attr_value(input)?;

            #[cfg(dbg_print)]
            println!("Dynamic attr: value = {:?}", attr_value);

            Ok((
                input,
                HtmlAttribute::Regular {
                    name: attr_name,
                    value: attr_value,
                },
            ))
        }
    }
}

fn parse_attr(input: &str) -> IResult<&str, HtmlAttribute> {
    let (input, attr) = alt((parse_directive, parse_vanilla_attr))(input)?;

    #[cfg(dbg_print)]
    {
        println!("Attribute: {:?}", attr);
        println!("Remaining input: {:?}", input);
    }

    Ok((input, attr))
}

pub fn parse_attributes(input: &str) -> IResult<&str, Vec<HtmlAttribute>> {
    many0(preceded(space1, parse_attr))(input)
}

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
