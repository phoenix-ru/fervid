extern crate nom;
use nom::branch::alt;
use nom::bytes::complete::{take_until1, take_until};
use nom::combinator::fail;
use nom::multi::many0;
use nom::sequence::{preceded, delimited};
use nom::{
  IResult,
  bytes::complete::tag,
  sequence::tuple
};
use std::str;

use crate::parser::html_utils::classify_element_kind;

use super::attributes::parse_attributes;
use super::html_utils::{html_name, space0};
use fervid_core::{ElementNode, StartingTag, Node, ElementKind, SfcBlock, SfcCustomBlock, HtmlAttribute, SfcTemplateBlock, SfcScriptBlock, SfcStyleBlock};

/// Parses the Vue Single-File Component
///
/// The Ok variant is a tuple, where:
/// - the `.0` element is the remaining input. It should be any trailing whitespace if parsing succeeded;
/// - the `.1` element is a vector of root blocks, i.e. all `<script>`, `<template>`, `<style>` and custom blocks.
///
/// This function does not modify whitespace inside the blocks.
///
/// To optimize template node, use [`crate::analyzer::ast_optimizer::optimize_ast`]
pub fn parse_sfc(input: &str) -> IResult<&str, Vec<SfcBlock>> {
  many0(parse_root_block)(input)
}

fn parse_root_block<'a>(input: &'a str) -> IResult<&'a str, SfcBlock<'a>> {
  // Remove leading space
  let input = input.trim_start();

  // Read starting tag
  let (input, starting_tag) = parse_element_starting_tag(input)?;

  // Shortcut
  let is_self_closing = starting_tag.is_self_closing;

  // Mutually exclusive flags
  let is_script = starting_tag.tag_name == "script";
  let is_template = !is_script && starting_tag.tag_name == "template";
  let is_style = !is_template && starting_tag.tag_name == "style";

  // Helper fn
  let read_raw_text = |input: &'a str| parse_text_node(input).map_or_else(
    move |_| (input, ""),
    move |v| {
      let Node::TextNode(content) = v.1 else {
        unreachable!("parse_text_node always returns a Node::TextNode")
      };
      (v.0, content)
    }
  );

  // Read custom blocks as raw text
  if !is_script && !is_template && !is_style {
    // Do not process anything if starting tag is self-closing
    if is_self_closing {
      return Ok((input, SfcBlock::Custom(SfcCustomBlock {
        starting_tag,
        content: ""
      })));
    }

    // Read raw text and end tag
    let (input, content) = read_raw_text(input);
    let (input, _end_tag) = parse_element_end_tag(input)?;

    return Ok((input, SfcBlock::Custom(SfcCustomBlock { starting_tag, content })));
  }

  // Get `lang` attribute, which is common for all the Vue root blocks
  let lang = starting_tag.attributes.iter().find_map(|attr| match attr {
    HtmlAttribute::Regular {
      name: "lang",
      value,
    } => Some(*value),
    _ => None,
  });

  // Parse children only for `<template>` root block
  if is_template {
    // Parser does not really look at it currently
    let lang = lang.unwrap_or("html");

    // Check for self-closing `<template />`. I see no reason why someone might do it, but still
    if is_self_closing {
      return Ok((input, SfcBlock::Template(SfcTemplateBlock {
        lang,
        roots: Vec::new()
      })));
    }

    // Parse children, this may Err (and is not handled yet)
    let (input, children) = parse_node_children(input)?;

    // End tag as well. No checks are present that it is the same
    // In the future, this checks may be implemented
    let (input, _end_tag) = parse_element_end_tag(input)?;

    return Ok((input, SfcBlock::Template(SfcTemplateBlock { lang, roots: children })));
  }

  // Read script
  if is_script {
    let lang = lang.unwrap_or("js");

    let is_setup = starting_tag
      .attributes
      .iter()
      .any(|attr| matches!(attr, HtmlAttribute::Regular { name: "setup", .. }));

    // Check self-closing
    if is_self_closing {
      return Ok((input, SfcBlock::Script(SfcScriptBlock { lang, content: "", is_setup })));
    }

    // Read raw text and end tag
    let (input, content) = read_raw_text(input);
    let (input, _end_tag) = parse_element_end_tag(input)?;

    return Ok((input, SfcBlock::Script(SfcScriptBlock { lang, content, is_setup })));
  }

  // Read style (basically same as script, but attributes are different)
  let lang = lang.unwrap_or("css");

  let is_scoped = starting_tag
    .attributes
    .iter()
    .any(|attr| matches!(attr, HtmlAttribute::Regular { name: "scoped", .. }));

  // Check self-closing
  if is_self_closing {
    return Ok((input, SfcBlock::Style(SfcStyleBlock { lang, content: "", is_scoped })));
  }

  // Read raw text and end tag (exactly like in script and custom blocks)
  let (input, content) = read_raw_text(input);
  let (input, _end_tag) = parse_element_end_tag(input)?;

  Ok((input, SfcBlock::Style(SfcStyleBlock { lang, content, is_scoped })))
}

fn parse_element_starting_tag(input: &str) -> IResult<&str, StartingTag> {
  let (input, (_, tag_name, attributes, _, ending_bracket)) = tuple((
    tag("<"),
    html_name,
    parse_attributes,
    space0,
    alt((tag(">"), tag("/>")))
  ))(input)?;

  #[cfg(dbg_print)]
  {
    println!("Tag name: {:?}", tag_name);
    println!("Attributes: {:?}", attributes);
  }

  Ok((input, StartingTag {
    tag_name,
    attributes,
    is_self_closing: ending_bracket == "/>",
    kind: classify_element_kind(&tag_name)
  }))
}

fn parse_element_end_tag(input: &str) -> IResult<&str, &str> {
  // eat any tag, because it may not match the start tag according to spec
  delimited(
    tag("</"),
    html_name,
    preceded(space0, tag(">"))
  )(input)
}

fn parse_dynamic_expression_node(input: &str) -> IResult<&str, Node> {
  let (input, expression_content) = parse_dynamic_expression(input)?;
  Ok((input, Node::DynamicExpression { value: expression_content.trim(), template_scope: 0 }))
}

// todo implement different processing ways:
// 1: parse node start and then recursively parse children
// 2: parse node start and seek the ending tag
pub fn parse_element_node(input: &str) -> IResult<&str, Node> {
  let (input, starting_tag) = parse_element_starting_tag(input)?;

  let early_return = matches!(starting_tag.kind, ElementKind::Void) || starting_tag.is_self_closing;

  if early_return {
    return Ok((
      input,
      Node::ElementNode(ElementNode {
        starting_tag,
        children: vec![],
        template_scope: 0
      })
    ));
  }

  let (input, children) = parse_node_children(input)?;

  // parse end tag
  let (input, end_tag) = parse_element_end_tag(input)?;

  // todo pass a stack of elements instead of a single tag
  // todo handle the error? soft/hard error -> either return Err or proceed and warn
  if end_tag != starting_tag.tag_name {
    println!("End tag does not match start tag: <{}> </{}>", &starting_tag.tag_name, &end_tag);
  }

  Ok((
    input,
    Node::ElementNode(ElementNode {
      starting_tag,
      children,
      template_scope: 0
    })
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

  Ok((
    input,
    Node::TextNode(text)
  ))
}

fn parse_comment_node(input: &str) -> IResult<&str, Node> {
  let (input, comment) = delimited(
    tag("<!--"),
    take_until("-->"),
    tag("-->")
  )(input)?;

  Ok((input, Node::CommentNode(comment)))
}

fn parse_node_children(input: &str) -> IResult<&str, Vec<Node>> {
  many0(alt((
    parse_dynamic_expression_node,
    parse_comment_node,
    parse_element_node,
    parse_text_node
  )))(input)
}
