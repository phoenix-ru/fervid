extern crate nom;
use nom::{
  IResult,
  bytes::complete::{
    tag,
    take_while,
    take_while1,
    take_till
  },
  character::{is_alphanumeric, is_space, complete::char},
  sequence::{preceded, delimited, tuple, separated_pair}, multi::many0
};

#[derive(PartialEq, Debug)]
pub struct StartingTag<'a> {
  tag_name: &'a str
}

#[derive(Debug)]
enum HtmlAttribute <'a> {
  Vanilla(&'a str, &'a str),
  Dynamic(&'a str, &'a str)
}

fn alpha(s: &[u8]) -> IResult<&[u8], &[u8]> {
  take_while1(is_alphanumeric)(s)
}

fn parse_html_name(input: &[u8]) -> IResult<&[u8], &[u8]> {
  // if !is_alphabetic(input[0]) {
  //   return Err(nom::Err::Error(nom::Err::Error(())))
  // };

  take_while(|x| is_alphanumeric(x) || x == b'-')(input)
}

fn parse_directive_name(input: &[u8]) -> IResult<&[u8], &[u8]> {
  preceded(
    tag("v-"),
    parse_html_name
  )(input)
}

fn parse_dynamic_attr(input: &[u8]) -> IResult<&[u8], HtmlAttribute> {
  let (input, ((directive_name, attr_name), attr_value)) = separated_pair(
    // todo implement v-bind directive shortcut `:smth`
    separated_pair(parse_directive_name, char(':'), parse_html_name),
    char('='),
    delimited(
      char('"'),
      take_till(|c| c == b'"'),
      char('"')
    )
  )(input)?;

  println!("Dynamic attr: directive = {:?}, name = {:?}, value = {:?}", directive_name, attr_name, attr_value);

  Ok((input, HtmlAttribute::Dynamic(std::str::from_utf8(attr_name).unwrap(), std::str::from_utf8(attr_value).unwrap())))
}

// fn parse_vanilla_attr(input: &[u8]) -> IResult<&[u8], HtmlAttribute> {

// }

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

  // println!("Attribute: {:?}", std::str::from_utf8(t2.1.0).unwrap_or(""));

  let dynamic = parse_dynamic_attr(input)?;
  println!("Attribute: {:?}", dynamic.1);
  println!("Remaining input: {:?}", std::str::from_utf8(dynamic.0).unwrap());

  Ok((dynamic.0, dynamic.1))

  /* Try parsing dynamic attrs which start with `v-` or `:` */
  // let prefix = alt((tag("v-"), tag(":")))(input);
  // let (input, prefix) = prefix.map_or(
  //   (input, ""),
  //   |x| (x.0, std::str::from_utf8(x.1).unwrap_or(""))
  // );

  // let (input, attr_name) = parse_html_name(input)?;

  // let (input, (attr_name, _, ))
}

fn parse_attributes(input: &[u8]) -> IResult<&[u8], Vec<HtmlAttribute>> {
  many0(preceded(
    take_while1(is_space),
    parse_attr
  ))(input)
}

pub fn parse_starting_tag(input: &[u8]) -> IResult<&[u8], StartingTag> {
  let (input, (_, tag_name, attrs, _)) = tuple((
    tag("<"),
    parse_html_name,
    /* Attr: start */
    parse_attributes,
    /* Attr: end */
    tag(">")
  ))(input)?;

  println!("Tag name: {:?}", tag_name);
  println!("Attr name: {:?}", attrs);

  Ok((input, StartingTag {
    tag_name: std::str::from_utf8(tag_name).unwrap()
  }))
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
