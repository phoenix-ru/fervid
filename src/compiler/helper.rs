pub struct CodeHelper <'a> {
  indent_level: usize,
  indent_str: &'a str
}

impl <'a> Default for CodeHelper<'a> {
  fn default() -> Self {
    CodeHelper { indent_level: 0, indent_str: "  " }
  }
}

impl <'a> CodeHelper <'a> {
  pub fn indent(self: &mut Self) {
    self.indent_level += 1
  }

  pub fn unindent(self: &mut Self) {
    self.indent_level -= 1
  }

  pub fn needs_escape(ident: &str) -> bool {
    ident.chars()
      .enumerate()
      .any(|(c_index, c)| {
        // Unescaped Js idents must not start with a number and must be ascii alphanumeric
        (c_index == 0 && !c.is_ascii_alphabetic()) || (c_index > 0 && !c.is_ascii_alphanumeric())
      })
  }

  pub fn newline(self: &Self, buf: &mut String) {
    buf.push('\n');

    // Add indent
    for _ in 0..self.indent_level {
      buf.push_str(self.indent_str)
    }
  }

  pub fn newline_size_hint(&self) -> usize {
    self.indent_level * self.indent_str.len()
  }

  pub fn newline_n(self: &Self, buf: &mut String, n: u8) {
    for _ in 0..n-1 {
      // empty lines should not be indented
      buf.push('\n');
    }

    if n > 1 {
      self.newline(buf);
    }
  }

  /// Writes a `: ` sequence
  pub fn colon(buf: &mut String) {
    buf.push_str(": ")
  }

  pub fn comma(buf: &mut String) {
    buf.push_str(", ")
  }

  pub fn comma_newline(&self, buf: &mut String) {
    buf.push(',');
    self.newline(buf)
  }

  pub fn null(buf: &mut String) {
    buf.push_str("null")
  }

  pub fn open_paren(buf: &mut String) {
    buf.push('(')
  }

  pub fn close_paren(buf: &mut String) {
    buf.push(')')
  }

  pub fn parens_option(buf: &mut String, v: Option<&str>) {
    Self::open_paren(buf);
    if let Some(value) = v {
      buf.push_str(value);
    }
    Self::close_paren(buf)
  }

  pub fn open_sq_bracket(buf: &mut String) {
    buf.push('[')
  }

  pub fn close_sq_bracket(buf: &mut String) {
    buf.push(']')
  }

  /// Generates a Js object from an iterator of (key, object)
  /// This will split an object across multiple lines if there are more than two properties.
  ///
  /// # Example
  /// Calling this function with `[("foo", "true"), ("1bar", "false")].iter()` would generate
  /// `{
  ///   foo: true,
  ///   "1bar": false
  /// }`.
  pub fn obj_from_entries_iter<'c>(&mut self, buf: &mut String, iter: impl Iterator<Item = (&'c str, &'c str)>) {
    let is_multiline = iter.size_hint().0 > 1;

    self.obj_open_paren(buf, is_multiline);

    for (index, (key, value)) in iter.enumerate() {
      if index > 0 && is_multiline {
        self.comma_newline(buf);
      } else if index > 0 {
        CodeHelper::comma(buf);
      }

      if Self::needs_escape(key) {
        CodeHelper::quoted(buf, key)
      } else {
        buf.push_str(key)
      }

      CodeHelper::colon(buf);
      buf.push_str(value);
    }

    self.obj_close_paren(buf, is_multiline);
  }

  /// Opens the Js object paren `{`.\
  /// Will indent and start a new line if `is_multiline` set to `true`
  pub fn obj_open_paren(&mut self, buf: &mut String, is_multiline: bool) {
    buf.push('{');
    if is_multiline {
      self.indent();
      self.newline(buf);
    }
  }

  /// Closes the Js object paren `}`.\
  /// Will unindent and add a new line if `is_multiline` set to `true`
  pub fn obj_close_paren(&mut self, buf: &mut String, is_multiline: bool) {
    if is_multiline {
      self.unindent();
      self.newline(buf);
    }

    buf.push('}');
  }

  pub fn quote(buf: &mut String) {
    buf.push('"')
  }

  pub fn quoted(buf: &mut String, v: &str) {
    Self::quote(buf);
    buf.push_str(v);
    Self::quote(buf)
  }

  /// Transforms a kebab-case identifier to camelCase
  pub fn to_camelcase(buf: &mut String, ident: &str) {
    buf.reserve(ident.len());

    for (index, word) in ident.split('-').enumerate() {
      if index == 0 {
        buf.push_str(word);
        continue;
      }

      let first_char = word.chars().next();
      if let Some(c) = first_char {
        buf.extend(c.to_uppercase());
        buf.push_str(&word[c.len_utf8()..]);
      }
    }
  }
}
