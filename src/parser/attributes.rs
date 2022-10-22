use nom::{
  IResult,
  bytes::complete::{take_while, take_till, tag},
  character::complete::char,
  sequence::{delimited, preceded}, branch::alt, multi::many0
};

use super::html_utils::{html_name, space1};

#[derive(Debug)]
pub enum HtmlAttribute <'a> {
  Regular(&'a str, &'a str),
  VDirective(VDirective<'a>, &'a str)
}

pub struct RegularAttribute <'a> {
  name: &'a str
}

#[derive(Debug)]
pub struct VDirective <'a> {
  name: &'a str,
  argument: &'a str,
  modifiers: Vec<&'a str>
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
    html_name(input)?
  } else {
    (input, "")
  };
  println!();
  println!("Parsed directive {:?}", directive_name);
  println!("Has argument: {}, argument: {:?}", has_argument, argument);

  /* Read modifiers */
  let (input, modifiers): (&str, Vec<&str>) = many0(preceded(
    char('.'),
    html_name
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
  let (input, attr_name) = html_name(input)?;

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

pub fn parse_attributes(input: &str) -> IResult<&str, Vec<HtmlAttribute>> {
  many0(preceded(
    space1,
    parse_attr
  ))(input)
}
