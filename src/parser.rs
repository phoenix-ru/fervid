extern crate nom;
use nom::{
  IResult,
  bytes::complete::tag,
  sequence::tuple
};
use std::str;

use self::attributes::parse_attributes;
use self::html_utils::{html_name, space0};

mod html_utils;
mod attributes;

#[derive(PartialEq, Debug)]
pub struct StartingTag<'a> {
  tag_name: &'a str
}

pub fn parse_node_starting(input: &str) -> IResult<&str, StartingTag> {
  let (input, (_, tag_name, attrs, _, _)) = tuple((
    tag("<"),
    html_name,
    /* Attr: start */
    parse_attributes,
    /* Attr: end */
    space0,
    tag(">")
  ))(input)?;

  println!("Tag name: {:?}", tag_name);
  println!("Attributes: {:?}", attrs);

  Ok((input, StartingTag {
    tag_name
  }))
}

// todo implement different processing ways:
// 1: parse node start and then recursively parse children
// 2: parse node start and seek the ending tag
pub fn parse_node(input: &str, parse_body: bool) -> IResult<&str, StartingTag> {
  parse_node_starting(input)
}
