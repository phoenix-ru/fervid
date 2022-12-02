pub struct CodeHelper <'a> {
  indent_level: usize,
  indent_str: &'a str
}

impl <'a> CodeHelper <'a> {
  pub fn indent(self: &mut Self) {
    self.indent_level += 1
  }

  pub fn unindent(self: &mut Self) {
    self.indent_level -= 1
  }

  pub fn newline(self: &Self, buf: &mut String) {
    buf.push('\n');

    // Add indent
    for _ in 0..self.indent_level {
      buf.push_str(self.indent_str)
    }
  }

  pub fn null(buf: &mut String) {
    buf.push_str("null")
  }

  pub fn comma(buf: &mut String) {
    buf.push_str(", ")
  }

  pub fn open_paren(buf: &mut String) {
    buf.push('(')
  }

  pub fn close_paren(buf: &mut String) {
    buf.push(')')
  }
}
