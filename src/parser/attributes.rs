use nom::{
  IResult,
  bytes::complete::{take_till, tag},
  character::complete::char,
  sequence::{delimited, preceded},
  branch::alt,
  multi::many0
};

use crate::parser::html_utils::{html_name, space1};

#[derive(Debug, Clone)]
pub enum HtmlAttribute <'a> {
  Regular {
    name: &'a str,
    value: &'a str
  },
  VDirective {
    name: &'a str,
    argument: &'a str,
    modifiers: Vec<&'a str>,
    value: Option<&'a str>,
    is_dynamic_slot: bool
  }
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
fn parse_directive(input: &str) -> IResult<&str, HtmlAttribute> {
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
  let mut is_dynamic_slot = false;
  let (input, argument) = if has_argument {
    // Support v-slot:[slotname]
    if directive_name == "slot" && input.starts_with("[") {
      is_dynamic_slot = true;

      delimited(char('['), html_name, char(']'))(input)?
    } else {
      html_name(input)?
    }
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

  Ok((input, HtmlAttribute::VDirective {
    name: directive_name,
    argument,
    modifiers,
    value: None,
    is_dynamic_slot
  }))
}

fn parse_dynamic_attr(input: &str) -> IResult<&str, HtmlAttribute> {
  let (input, directive) = parse_directive(input)?;
  println!("Dynamic attr: directive = {:?}", directive);

  /* Try taking a `=` char, early return if it's not there */
  if !input.starts_with('=') {
    return Ok((input, directive));
  }

  let (input, attr_value) = parse_attr_value(&input[1..])?;

  println!("Dynamic attr: value = {:?}", attr_value);

  match directive {
    HtmlAttribute::VDirective { name, argument, modifiers, is_dynamic_slot, .. } => Ok((input, HtmlAttribute::VDirective {
      name,
      argument,
      modifiers,
      value: Some(attr_value),
      is_dynamic_slot
    })),

    /* Not possible, because parse_directive returns a directive indeed */
    _ => Err(nom::Err::Error(nom::error::Error {
      code: nom::error::ErrorKind::Fail,
      input
    }))
  }
}

fn parse_vanilla_attr(input: &str) -> IResult<&str, HtmlAttribute> {
  let (input, attr_name) = html_name(input)?;

  /* Support omitting a `=` char */
  let eq: Result<(&str, char), nom::Err<nom::error::Error<_>>> = char('=')(input);
  match eq {
    // consider omitted attribute as attribute name itself (as current Vue compiler does)
    Err(_) => Ok((input, HtmlAttribute::Regular {
      name: attr_name,
      value: &attr_name
    })),

    Ok((input, _)) => {
      let (input, attr_value) = parse_attr_value(input)?;
  
      println!("Dynamic attr: value = {:?}", attr_value);
  
      Ok((input, HtmlAttribute::Regular {
        name: attr_name,
        value: attr_value
      }))
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
