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

fn parse_html_name(input: &[u8]) -> IResult<&[u8], &[u8]> {
  // if !is_alphabetic(input[0]) {
  //   return Err(nom::Err::Error(nom::Err::Error(())))
  // };

  take_while(|x| is_alphanumeric(x) || x == b'-')(input)
}

fn parse_attr_value(input: &[u8]) -> IResult<&[u8], &[u8]> {
  delimited(
    char('"'),
    take_till(|c| c == b'"'),
    char('"')
  )(input)
}

/**
 Parses a directive in form of `v-directive-name:directive-attribute.modifier1.modifier2`

 Allows for shortcuts like `@` (same as `v-on`), `:` (`v-bind`) and `#` (`v-slot`)
 */
fn parse_directive(input: &[u8]) -> IResult<&[u8], VDirective> {
  let (input, prefix) = alt((tag("v-"), tag("@"), tag("#"), tag(":")))(input)?;

  /* Determine directive name */
  let mut has_argument = false;
  let (input, directive_name) = match prefix {
    b"v-" => {
      let (input, name) = parse_html_name(input)?;

      // next char is colon, shift input and set flag
      if let Some(b':') = input.get(0) {
        has_argument = true;
        (&input[1..], name)
      } else {
        (input, name)
      }
    }

    b"@" => {
      has_argument = true;
      (input, b"on" as &[u8])
    }

    b":" => {
      has_argument = true;
      (input, b"bind" as &[u8])
    }

    b"#" => {
      has_argument = true;
      (input, b"slot" as &[u8])
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
    parse_html_name(input)?
  } else {
    (input, b"" as &[u8])
  };
  println!();
  println!("Parsed directive {:?}", str::from_utf8(directive_name).unwrap());
  println!("Has argument: {}, argument: {:?}", has_argument, str::from_utf8(argument).unwrap());

  /* Read modifiers */
  let (input, modifiers) = many0(preceded(
    char('.'),
    parse_html_name
  ))(input).unwrap_or((input, vec![]));

  Ok((input, VDirective {
    name: str::from_utf8(directive_name).unwrap_or("error"), // can it fail?
    argument: str::from_utf8(argument).unwrap(), // can this fail???
    modifiers: modifiers.iter().map(|&x| str::from_utf8(x).unwrap()).collect() // can this fail???
  }))
}

fn parse_dynamic_attr(input: &[u8]) -> IResult<&[u8], HtmlAttribute> {
  let (input, directive) = parse_directive(input)?;
  println!("Dynamic attr: directive = {:?}", directive);

  /* Try taking a `=` char, early return if it's not there */
  let eq: Result<(&[u8], char), nom::Err<nom::error::Error<_>>> = char('=')(input);
  match eq {
    Err(_) => Ok((input, HtmlAttribute::VDirective(directive, ""))),

    Ok((input, _)) => {
      let (input, attr_value) = parse_attr_value(input)?;
  
      println!("Dynamic attr: value = {:?}", str::from_utf8(attr_value).unwrap());
  
      Ok((input, HtmlAttribute::VDirective(
        directive,
        str::from_utf8(attr_value).unwrap()
      )))
    }
  }
}

fn parse_vanilla_attr(input: &[u8]) -> IResult<&[u8], HtmlAttribute> {
  let (input, attr_name) = parse_html_name(input)?;
  let attr_name = str::from_utf8(attr_name).unwrap();

  /* Support omitting a `=` char */
  let eq: Result<(&[u8], char), nom::Err<nom::error::Error<_>>> = char('=')(input);
  match eq {
    // consider omitted attribute as attribute name itself (as current Vue compiler does)
    Err(_) => Ok((input, HtmlAttribute::Regular(attr_name, &attr_name))),

    Ok((input, _)) => {
      let (input, attr_value) = parse_attr_value(input)?;
  
      println!("Dynamic attr: value = {:?}", str::from_utf8(attr_value).unwrap());
  
      Ok((input, HtmlAttribute::Regular(
        attr_name,
        str::from_utf8(attr_value).unwrap()
      )))
    }
  }
}

fn parse_attr(input: &[u8]) -> IResult<&[u8], HtmlAttribute> {
  // let t2 = consumed(
  //   value(
  //     true, 
  //     separated_pair(
  //       preceded(tag("v-"), parse_html_name),
  //       char(':'),
  //       parse_html_name
  //     )
  //   )
  // )(input)?;

  // println!("Attribute: {:?}", str::from_utf8(t2.1.0).unwrap_or(""));

  let (input, attr) = alt((parse_dynamic_attr, parse_vanilla_attr))(input)?;

  println!("Attribute: {:?}", attr);
  println!("Remaining input: {:?}", str::from_utf8(input).unwrap());

  Ok((input, attr))

  /* Try parsing dynamic attrs which start with `v-` or `:` */
  // let prefix = alt((tag("v-"), tag(":")))(input);
  // let (input, prefix) = prefix.map_or(
  //   (input, ""),
  //   |x| (x.0, str::from_utf8(x.1).unwrap_or(""))
  // );

  // let (input, attr_name) = parse_html_name(input)?;

  // let (input, (attr_name, _, ))
}

fn parse_attributes(input: &[u8]) -> IResult<&[u8], Vec<HtmlAttribute>> {
  many0(preceded(
    whitespace1,
    parse_attr
  ))(input)
}

pub fn parse_starting_tag(input: &[u8]) -> IResult<&[u8], StartingTag> {
  let (input, (_, tag_name, attrs, _, _)) = tuple((
    tag("<"),
    parse_html_name,
    /* Attr: start */
    parse_attributes,
    /* Attr: end */
    whitespace0,
    tag(">")
  ))(input)?;

  println!("Tag name: {:?}", tag_name);
  println!("Attributes: {:?}", attrs);

  Ok((input, StartingTag {
    tag_name: str::from_utf8(tag_name).unwrap()
  }))
}

fn whitespace1(input: &[u8]) -> IResult<&[u8], &[u8]> {
  take_while1(|x| is_space(x) || is_newline(x))(input)
}

fn whitespace0(input: &[u8]) -> IResult<&[u8], &[u8]> {
  take_while(|x| is_space(x) || is_newline(x))(input)
}

#[test]
fn parse() {
  let e: &[u8] = b"";

  let test_fissure = include_bytes!("./test/input.vue");

  assert_eq!(
    parse_starting_tag(test_fissure),
    Ok((e, StartingTag {
      tag_name: "abc-def"
    }))
  );
}
