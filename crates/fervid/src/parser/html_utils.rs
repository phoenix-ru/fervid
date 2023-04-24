use fervid_core::ElementKind;
use nom::{bytes::complete::{take_while1, take_while}, IResult};

// According to https://www.w3.org/TR/2011/WD-html5-20110525/syntax.html#elements-0
const VOID_TAGS: [&str; 16] = ["area", "base", "br", "col", "command", "embed", "hr", "img", "input", "keygen", "link", "meta", "param", "source", "track", "wbr"];
const RAW_TEXT_ELEMENTS: [&str; 2] = ["script", "style"];
const RCDATA_ELEMENTS: [&str; 2] = ["textarea", "title"];
const FOREIGN_ELEMENTS: [&str; 1] = ["svg"]; // todo

pub fn classify_element_kind(tag_name: &str) -> ElementKind {
  let tag_lowercase = &tag_name.to_lowercase();
  let tag_lowercase = tag_lowercase.as_str();
  if RCDATA_ELEMENTS.contains(&tag_lowercase) {
    ElementKind::RCData
  } else if FOREIGN_ELEMENTS.contains(&tag_lowercase) {
    ElementKind::Foreign
  } else if RAW_TEXT_ELEMENTS.contains(&tag_lowercase) {
    ElementKind::RawText
  } else if VOID_TAGS.contains(&tag_lowercase) {
    ElementKind::Void
  } else {
    ElementKind::Normal
  }
}

/**
 * U+0020 SPACE, U+0009 CHARACTER TABULATION (tab), U+000A LINE FEED (LF), U+000C FORM FEED (FF), and U+000D CARRIAGE RETURN (CR)
 * https://www.w3.org/TR/2011/WD-html5-20110525/common-microsyntaxes.html#space-character
 */
pub fn is_space_char(x: char) -> bool {
  x == ' ' || x == '\t' || x == '\n' || x == '\r' || x == '\u{000C}'
}

// todo allow more symbols as per W3 spec
pub fn is_valid_name_char(x: char) -> bool {
  // (x >= 'A' && x <= 'Z') || (x >= 'a' && x <= 'z')
  x.is_alphabetic()
}

pub fn html_name(input: &str) -> IResult<&str, &str> {
  // todo control dashes?? allow unicode??
  take_while1(|x: char| is_valid_name_char(x) || x == '-')(input)
}

pub fn space1(input: &str) -> IResult<&str, &str> {
  take_while1(is_space_char)(input)
}

pub fn space0(input: &str) -> IResult<&str, &str> {
  take_while(is_space_char)(input)
}
