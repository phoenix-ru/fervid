extern crate nom;
use nom::branch::alt;
use nom::bytes::complete::{take_until, take_until1};
use nom::combinator::fail;
use nom::error::{ErrorKind, ParseError};
use nom::multi::many0;
use nom::sequence::{delimited, preceded};
use nom::{bytes::complete::tag, sequence::tuple, IResult};
use std::str;
use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::Module;

use super::attributes::parse_attributes;
use super::ecma::{parse_js, parse_js_module};
use super::html_utils::{classify_element_kind, html_name, space0, TagKind};
use fervid_core::{
    fervid_atom, AttributeOrBinding, ElementKind, ElementNode, FervidAtom, Interpolation, Node,
    SfcCustomBlock, SfcDescriptor, SfcScriptBlock, SfcScriptLang, SfcStyleBlock, SfcTemplateBlock,
    StartingTag,
};

/// Parses the Vue Single-File Component
///
/// The Ok variant is a tuple, where:
/// - the `.0` element is the remaining input. It should be any trailing whitespace if parsing succeeded;
/// - the `.1` element is a vector of root blocks, i.e. all `<script>`, `<template>`, `<style>` and custom blocks.
///
/// This function does not modify whitespace inside the blocks.
///
/// To optimize template node, use the `fervid_transform` crate
pub fn parse_sfc(mut input: &str) -> IResult<&str, SfcDescriptor> {
    let mut result = SfcDescriptor::default();

    // Adapted from Nom's `many0`
    loop {
        let len = input.len();
        match parse_root_block(input, &mut result) {
            Err(nom::Err::Error(_)) => return Ok((input, result)),
            Err(e) => return Err(e),
            Ok(new_input) => {
                // infinite loop check: the parser must always consume
                if new_input.len() == len {
                    return Err(nom::Err::Error(ParseError::from_error_kind(
                        input,
                        ErrorKind::Many0,
                    )));
                }

                input = new_input;
            }
        }
    }
}

fn parse_root_block<'a>(
    input: &'a str,
    out: &mut SfcDescriptor,
) -> Result<&'a str, nom::Err<nom::error::Error<&'a str>>> {
    // Remove leading space
    let input = input.trim_start();

    // Read starting tag
    let (input, (starting_tag, is_self_closing)) = parse_element_starting_tag(input)?;

    // Mutually exclusive flags
    let is_script = starting_tag.tag_name == "script";
    let is_template = !is_script && starting_tag.tag_name == "template";
    let is_style = !is_template && starting_tag.tag_name == "style";

    // Helper fn
    let read_raw_text = |input: &'a str| {
        parse_text_node(input).map_or_else(
            move |_| (input, "".into()),
            move |v| {
                let Node::Text(content, _) = v.1 else {
                    unreachable!("parse_text_node always returns a Node::TextNode")
                };
                (v.0, content)
            },
        )
    };

    // Read custom blocks as raw text
    if !is_script && !is_template && !is_style {
        // Do not process anything if starting tag is self-closing
        if is_self_closing {
            out.custom_blocks.push(SfcCustomBlock {
                starting_tag,
                content: "".into(),
                span: DUMMY_SP,
            });

            return Ok(input);
        }

        // Read raw text and end tag
        let (input, content) = read_raw_text(input);
        let (input, _end_tag) = parse_element_end_tag(input)?;

        out.custom_blocks.push(SfcCustomBlock {
            starting_tag,
            content,
            span: DUMMY_SP,
        });

        return Ok(input);
    }

    // Get `lang` attribute, which is common for all the Vue root blocks
    let lang = starting_tag.attributes.iter().find_map(|attr| match attr {
        AttributeOrBinding::RegularAttribute { name, value, .. } if name == "lang" => {
            Some(value.to_owned())
        }
        _ => None,
    });

    // Parse children only for `<template>` root block
    if is_template {
        // Parser does not really look at it currently
        let lang = lang.unwrap_or(fervid_atom!("html"));

        // Check for self-closing `<template />`. I see no reason why someone might do it, but still
        if is_self_closing {
            out.template = Some(SfcTemplateBlock {
                lang,
                roots: Vec::new(),
                span: DUMMY_SP, // TODO
            });

            return Ok(input);
        }

        // Parse children, this may Err (and is not handled yet)
        let (input, children) = parse_node_children(input)?;

        // End tag as well. No checks are present that it is the same
        // In the future, this checks may be implemented
        let (input, _end_tag) = parse_element_end_tag(input)?;

        out.template = Some(SfcTemplateBlock {
            lang,
            roots: children,
            span: DUMMY_SP, // TODO
        });

        return Ok(input);
    }

    // Read script
    if is_script {
        let lang = if matches!(lang.as_deref(), Some("ts")) {
            SfcScriptLang::Typescript
        } else {
            SfcScriptLang::Es
        };

        let is_setup = starting_tag.attributes.iter().any(|attr| {
            matches!(
                attr,
                AttributeOrBinding::RegularAttribute { name, .. } if name.eq("setup")
            )
        });

        macro_rules! add_script {
            ($content: expr) => {
                if is_setup {
                    out.script_setup = Some(SfcScriptBlock {
                        content: $content,
                        lang,
                        is_setup,
                        span: DUMMY_SP,
                    });
                } else {
                    out.script_legacy = Some(SfcScriptBlock {
                        content: $content,
                        lang,
                        is_setup,
                        span: DUMMY_SP,
                    })
                }
            };
        }

        // Check self-closing
        if is_self_closing {
            // Include the empty script. Maybe ignore them in the future?
            add_script!(Box::new(Module {
                span: DUMMY_SP,
                body: vec![],
                shebang: None,
            }));
            return Ok(input);
        }

        // Read raw text and end tag
        let (input, content) = read_raw_text(input);
        let (input, _end_tag) = parse_element_end_tag(input)?;

        // TODO Span
        let content = parse_js_module(&content, 0, 0);
        match content {
            Ok(content_module) => {
                add_script!(Box::new(content_module));
                return Ok(input);
            }
            Err(_) => {
                return Err(nom::Err::Error(nom::error::Error {
                    input,
                    code: ErrorKind::Fail,
                }));
            }
        }
    }

    // Read style (basically same as script, but attributes are different)
    let lang = lang.unwrap_or_else(|| FervidAtom::from("css"));

    let is_scoped = starting_tag.attributes.iter().any(|attr| {
        matches!(
            attr,
            AttributeOrBinding::RegularAttribute { name, .. } if name.eq("scoped")
        )
    });
    let is_module = starting_tag.attributes.iter().any(|attr| {
        matches!(
            attr,
            AttributeOrBinding::RegularAttribute { name, .. } if name.eq("module")
        )
    });

    // Check self-closing, ignore such styles
    if is_self_closing {
        return Ok(input);
    }

    // Read raw text and end tag (exactly like in script and custom blocks)
    let (input, content) = read_raw_text(input);
    let (input, _end_tag) = parse_element_end_tag(input)?;

    out.styles.push(SfcStyleBlock {
        lang,
        content,
        is_scoped,
        is_module,
        span: DUMMY_SP,
    });

    Ok(input)
}

fn parse_element_starting_tag(input: &str) -> IResult<&str, (StartingTag, bool)> {
    let (input, (_, tag_name, (attributes, directives), _, ending_bracket)) = tuple((
        tag("<"),
        html_name,
        parse_attributes,
        space0,
        alt((tag(">"), tag("/>"))),
    ))(input)?;

    #[cfg(feature = "dbg_print")]
    {
        println!("Tag name: {:?}", tag_name);
        println!("Attributes: {:?}", attributes);
    }

    Ok((
        input,
        (
            StartingTag {
                tag_name: FervidAtom::from(tag_name),
                attributes,
                directives,
            },
            // is_self_closing
            ending_bracket == "/>",
        ),
    ))
}

fn parse_element_end_tag(input: &str) -> IResult<&str, &str> {
    // eat any tag, because it may not match the start tag according to spec
    delimited(tag("</"), html_name, preceded(space0, tag(">")))(input)
}

fn parse_interpolation_node(input: &str) -> IResult<&str, Node> {
    let (input, raw_expression) = parse_dynamic_expression(input)?;

    // TODO Span
    let parsed = match parse_js(raw_expression, 0, 0) {
        Ok(parsed) => parsed,
        Err(_) => return fail(input), // this ignores the bad interpolation (unconfirmed)
    };

    Ok((
        input,
        Node::Interpolation(Interpolation {
            value: parsed,
            template_scope: 0,
            patch_flag: false,
            span: DUMMY_SP,
        }),
    ))
}

// todo implement different processing ways:
// 1: parse node start and then recursively parse children
// 2: parse node start and seek the ending tag
pub fn parse_element_node(input: &str) -> IResult<&str, Node> {
    let (input, (starting_tag, is_self_closing)) = parse_element_starting_tag(input)?;

    let element_kind = classify_element_kind(&starting_tag.tag_name);

    let early_return = matches!(element_kind, TagKind::Void) || is_self_closing;

    if early_return {
        return Ok((
            input,
            Node::Element(ElementNode {
                starting_tag,
                children: vec![],
                template_scope: 0,
                kind: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP, // TODO
            }),
        ));
    }

    let (input, children) = parse_node_children(input)?;

    // parse end tag
    let (input, end_tag) = parse_element_end_tag(input)?;

    // todo pass a stack of elements instead of a single tag
    // todo handle the error? soft/hard error -> either return Err or proceed and warn
    if !starting_tag.tag_name.eq(end_tag) {
        println!(
            "End tag does not match start tag: <{}> </{}>",
            &starting_tag.tag_name, &end_tag
        );
    }

    Ok((
        input,
        Node::Element(ElementNode {
            starting_tag,
            children,
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP, // TODO
        }),
    ))
}

// parses {{ expression }}
fn parse_dynamic_expression(input: &str) -> IResult<&str, &str> {
    delimited(tag("{{"), take_until1("}}"), tag("}}"))(input)
}

fn parse_text_node(input: &str) -> IResult<&str, Node> {
    let mut iter = input.chars().peekable();
    let mut bytes_taken = 0;

    while let Some(ch) = iter.next() {
        if ch == '<' {
            break;
        };
        // todo support other delimiters
        if let ('{', Some('{')) = (ch, iter.peek()) {
            break;
        };
        bytes_taken += ch.len_utf8();
    }

    /* Return error if input length is too short */
    if bytes_taken == 0 {
        return fail(input);
    }

    let (text, input) = input.split_at(bytes_taken);

    Ok((input, Node::Text(text.into(), DUMMY_SP)))
}

fn parse_comment_node(input: &str) -> IResult<&str, Node> {
    let (input, comment) = delimited(tag("<!--"), take_until("-->"), tag("-->"))(input)?;

    Ok((input, Node::Comment(comment.into(), DUMMY_SP)))
}

fn parse_node_children(input: &str) -> IResult<&str, Vec<Node>> {
    many0(alt((
        parse_interpolation_node,
        parse_comment_node,
        parse_element_node,
        parse_text_node,
    )))(input)
}
