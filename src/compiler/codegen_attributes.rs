use crate::parser::attributes::HtmlAttribute;
use super::codegen::CodegenContext;
use super::imports::VueImports;

impl CodegenContext <'_> {
  pub fn generate_attributes(self: &mut Self, buf: &mut String, attributes: &Vec<HtmlAttribute>) -> bool {
    /* Work is not needed if we don't have any Regular attributes, v-on/v-bind directives */
    if !has_attributes_work(attributes) {
      return false;
    }

    // Start a Js object notation
    buf.push('{');
    self.code_helper.indent();
    self.code_helper.newline(buf);

    /* For adding a comma `,` */
    let mut has_first_element = false;

    /* For `withModifiers` import */
    let mut has_used_modifiers_import = false;

    for attribute in attributes {
      if has_first_element {
        self.code_helper.comma_newline(buf);
      }

      match attribute {
        HtmlAttribute::Regular { name, value } => {
          /* For obvious reasons, attributes containing a dash need to be escaped */
          let needs_quotes = name.contains('-');

          /* "attr-name": "attr value" */
          if needs_quotes { buf.push('"'); }
          buf.push_str(name);
          if needs_quotes { buf.push('"'); }
          buf.push_str(": \"");
          buf.push_str(value);
          buf.push('"');

          // todo special handling for `style` attribute
          // todo special handling for `class` attribute (because of :class)

          has_first_element = true
        },

        // todo generate attributes bound with v-bind, v-on
        HtmlAttribute::VDirective { name, argument, modifiers, value, .. } => {
          /* v-on directive, shortcut `@`, e.g. `@custom-event.modifier="value"` */
          if *name == "on" {
            generate_v_on_attr(buf, argument, modifiers, *value);

            has_used_modifiers_import |= modifiers.len() > 0;
            has_first_element = true
          }

          /* v-bind directive, shortcut `:`, e.g. `:custom-prop="value"` */
          else if *name == "bind" {
            generate_v_bind_attr(buf, argument, *value);

            has_first_element = true
          }
        }
      }
    }

    // Close a Js object notation
    self.code_helper.unindent();
    self.code_helper.newline(buf);
    buf.push('}');

    /* Add imports */
    if has_used_modifiers_import {
      self.add_to_imports(VueImports::WithModifiers);
    }

    true
  }
}

pub fn has_attributes_work(attributes: &Vec<HtmlAttribute>) -> bool {
  /* Work is not needed if we don't have any Regular attributes, v-on/v-bind directives */
  attributes.iter().any(|it| match it {
    HtmlAttribute::Regular { .. } => true,
    HtmlAttribute::VDirective { name, .. } => match *name {
      "on" | "bind" => true,
      _ => false
    }
  })
}

fn generate_v_bind_attr(buf: &mut String, argument: &str, value: Option<&str>) {
  // todo what to do when you have the same regular attribute??
  // I don't want to do O(n^2) search for every attribute, but I also don't want to allocate any extra memory for a vec/set
  // maybe the analysis lib should report such issues??
  // current js SFC compiler just discards the second appearance of the same attribute name (except for class)

  /* Do not generate empty values */
  let value_expression: &str;
  if let Some(v) = value {
    value_expression = v
  } else {
    // At this point js SFC compiler just panicks
    return
  }

  /* For obvious reasons, attributes containing a dash need to be escaped */
  let needs_quotes = argument.contains('-');

  /* "attr-name": attr_expression */
  if needs_quotes { buf.push('"'); }
  buf.push_str(argument);
  if needs_quotes { buf.push('"'); }
  buf.push_str(": ");
  buf.push_str(value_expression); // todo use analysis and add _ctx
}

/// Generates the code for a v-on listener
/// - buf is where to write the resulting code
/// - argument is the name of the event to listen to (e.g. `click` in `@click`)
/// - modifiers are event modifiers (e.g. `stop`, `prevent` in `@click.stop.prevent`)
/// - value is the value of the listener, typically an expression (e.g. `doSmth` in `@click="doSmth"`)
fn generate_v_on_attr(buf: &mut String, argument: &str, modifiers: &Vec<&str>, value: Option<&str>) {
  /* Transform name of event to camelCase, e.g. `onCustomEventName` */
  buf.push_str("on");
  for word in argument.split('-') {
    let first_char = word.chars().next();
    if let Some(ch) = first_char {
      /* Uppercase the first char and append to buf */
      for ch_component in ch.to_uppercase() {
        buf.push(ch_component);
      }

      /* Push the rest of the word */
      buf.push_str(&word[ch.len_utf8()..]);
    }
  }

  buf.push_str(": ");

  /* Modifiers present: add function call */
  if modifiers.len() > 0 {
    buf.push_str(CodegenContext::get_import_str(VueImports::WithModifiers));
    buf.push('(');
  }

  /* Value may be absent, e.g. `@click.stop` */
  if let Some(v) = value {
    // todo use _ctx
    buf.push_str(v)
  } else {
    // todo use _cache
    buf.push_str("() => {}")
  }

  /* Modifiers present: add them in a Js array of strings */
  if modifiers.len() > 0 {
    buf.push_str(", [");

    for (index, modifier) in modifiers.iter().enumerate() {
      if index > 0 {
        buf.push_str(", ");
      }

      buf.push('"');
      buf.push_str(modifier);
      buf.push('"');
    }

    /* Close both an array and a `_withModifiers` function call */
    buf.push_str("])")
  }
}

#[test]
fn test_attributes_generation() {
  let initial_buf = "fn(arg1, ";
  let mut buf = String::from(initial_buf);
  let mut ctx: CodegenContext = Default::default();

  let attributes = vec![
    HtmlAttribute::VDirective { name: "test", argument: "", modifiers: vec![], value: None, is_dynamic_slot: false },
    HtmlAttribute::Regular { name: "class", value: "test" },
    HtmlAttribute::Regular { name: "readonly", value: "" },
    HtmlAttribute::Regular { name: "style", value: "background: red" },
    HtmlAttribute::Regular { name: "dashed-attr-name", value: "spaced attr value" }
  ];

  ctx.generate_attributes(&mut buf, &attributes);
  assert_eq!(
    &buf,
    &format!(r#"{initial_buf}{{class: "test", readonly: "", style: "background: red", "dashed-attr-name": "spaced attr value"}}"#)
  );
}
