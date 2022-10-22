extern crate nom;
use nom::branch::alt;
use nom::{
  IResult,
  bytes::complete::tag,
  sequence::tuple
};
use std::str;

use crate::parser::html_utils::classify_element_kind;

use self::attributes::{parse_attributes, HtmlAttribute};
use self::html_utils::{html_name, space0, ElementKind};

mod html_utils;
mod attributes;

#[derive(Debug)]
pub struct StartingTag<'a> {
  tag_name: &'a str,
  attributes: Vec<HtmlAttribute<'a>>,
  is_self_closing: bool,
  kind: ElementKind
}

pub fn parse_element_starting_tag(input: &str) -> IResult<&str, StartingTag> {
  let (input, (_, tag_name, attributes, _, ending_bracket)) = tuple((
    tag("<"),
    html_name,
    parse_attributes,
    space0,
    alt((tag(">"), tag("/>")))
  ))(input)?;

  println!("Tag name: {:?}", tag_name);
  println!("Attributes: {:?}", attributes);

  Ok((input, StartingTag {
    tag_name,
    attributes,
    is_self_closing: ending_bracket == "/>",
    kind: classify_element_kind(&tag_name)
  }))
}

// todo implement different processing ways:
// 1: parse node start and then recursively parse children
// 2: parse node start and seek the ending tag
pub fn parse_node(input: &str, parse_body: bool) -> IResult<&str, StartingTag> {
  parse_element_starting_tag(input)
}
