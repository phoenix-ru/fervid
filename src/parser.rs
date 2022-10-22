extern crate nom;
use nom::{
  IResult,
  branch::alt,
  bytes::complete::{
    tag,
    take_while,
    take_while1,
    take_till, take
  },
  character::{is_alphanumeric, is_space, complete::char, is_newline},
  sequence::{preceded, delimited, tuple, separated_pair},
  multi::many0
};
use std::str;

use self::html_utils::{is_valid_name_char, is_space_char};

mod html_utils;

#[derive(PartialEq, Debug)]
pub struct StartingTag<'a> {
  tag_name: &'a str
}

#[derive(Debug)]
enum HtmlAttribute <'a> {
  Regular(&'a str, &'a str),
  VDirective(VDirective<'a>, &'a str)
}

struct RegularAttribute <'a> {
  name: &'a str
}

#[derive(Debug)]
struct VDirective <'a> {
  name: &'a str,
  argument: &'a str,
  modifiers: Vec<&'a str>
}

fn parse_html_name_chars(input: &str) -> IResult<&str, &str> {
  // todo control dashes??
  take_while(|x: char| is_valid_name_char(x) || x == '-')(input)
}

fn parse_attr_value(input: &str) -> IResult<&str, &str> {
  delimited(
    char('"'),
    take_till(|c| c == '"'),
    char('"')
  )(input)
}

/**
 Parses a directive in form of `v-directive-name:directive-attribute.modifier1.modifier2`

 Allows for shortcuts like `@` (same as `v-on`), `:` (`v-bind`) and `#` (`v-slot`)
 */
fn parse_directive(input: &str) -> IResult<&str, VDirective> {
  let (input, prefix) = alt((tag("v-"), tag("@"), tag("#"), tag(":")))(input)?;

  /* Determine directive name */
  let mut has_argument = false;
  let (input, directive_name) = match prefix {
    "v-" => {
      let (input, name) = parse_html_name_chars(input)?;

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

    "#" => {
      has_argument = true;
      (input, "slot")
    }

    _ => {
      return Err(nom::Err::Error(nom::error::Error {
        code: nom::error::ErrorKind::Tag,
        input
      }))
    }
  };

  /* Read argument part if we spotted `:` earlier */
  let (input, argument) = if has_argument {
    parse_html_name_chars(input)?
  } else {
    (input, "")
  };
  println!();
  println!("Parsed directive {:?}", directive_name);
  println!("Has argument: {}, argument: {:?}", has_argument, argument);

  /* Read modifiers */
  let (input, modifiers): (&str, Vec<&str>) = many0(preceded(
    char('.'),
    parse_html_name_chars
  ))(input).unwrap_or((input, vec![]));

  Ok((input, VDirective {
    name: directive_name,
    argument,
    modifiers
  }))
}

fn parse_dynamic_attr(input: &str) -> IResult<&str, HtmlAttribute> {
  let (input, directive) = parse_directive(input)?;
  println!("Dynamic attr: directive = {:?}", directive);

  /* Try taking a `=` char, early return if it's not there */
  let eq: Result<(&str, char), nom::Err<nom::error::Error<_>>> = char('=')(input);
  match eq {
    Err(_) => Ok((input, HtmlAttribute::VDirective(directive, ""))),

    Ok((input, _)) => {
      let (input, attr_value) = parse_attr_value(input)?;
  
      println!("Dynamic attr: value = {:?}", attr_value);
  
      Ok((input, HtmlAttribute::VDirective(
        directive,
        attr_value
      )))
    }
  }
}

fn parse_vanilla_attr(input: &str) -> IResult<&str, HtmlAttribute> {
  let (input, attr_name) = parse_html_name_chars(input)?;
  let attr_name = attr_name;

  /* Support omitting a `=` char */
  let eq: Result<(&str, char), nom::Err<nom::error::Error<_>>> = char('=')(input);
  match eq {
    // consider omitted attribute as attribute name itself (as current Vue compiler does)
    Err(_) => Ok((input, HtmlAttribute::Regular(attr_name, &attr_name))),

    Ok((input, _)) => {
      let (input, attr_value) = parse_attr_value(input)?;
  
      println!("Dynamic attr: value = {:?}", attr_value);
  
      Ok((input, HtmlAttribute::Regular(
        attr_name,
        attr_value
      )))
    }
  }
}

fn parse_attr(input: &str) -> IResult<&str, HtmlAttribute> {
  let (input, attr) = alt((parse_dynamic_attr, parse_vanilla_attr))(input)?;

  println!("Attribute: {:?}", attr);
  println!("Remaining input: {:?}", input);

  Ok((input, attr))
}

fn parse_attributes(input: &str) -> IResult<&str, Vec<HtmlAttribute>> {
  many0(preceded(
    space1,
    parse_attr
  ))(input)
}

pub fn parse_node_starting(input: &str) -> IResult<&str, StartingTag> {
  let (input, (_, tag_name, attrs, _, _)) = tuple((
    tag("<"),
    parse_html_name_chars,
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

fn space1(input: &str) -> IResult<&str, &str> {
  take_while1(is_space_char)(input)
}

fn space0(input: &str) -> IResult<&str, &str> {
  take_while(is_space_char)(input)
}
