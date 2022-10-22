const VOID_TAGS: [&str; 16] = ["area", "base", "br", "col", "command", "embed", "hr", "img", "input", "keygen", "link", "meta", "param", "source", "track", "wbr"];

pub fn is_void_element(tag_name: &str) -> bool {
  VOID_TAGS.contains(&tag_name)
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
  (x >= 'A' && x <= 'Z') || (x >= 'a' && x <= 'z')
}
