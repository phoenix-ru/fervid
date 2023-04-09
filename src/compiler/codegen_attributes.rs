use lazy_static::lazy_static;
use regex::Regex;
use crate::parser::attributes::{HtmlAttribute, VDirective};
use super::codegen::CodegenContext;
use super::helper::CodeHelper;
use super::imports::VueImports;
use super::transform::swc::transform_scoped;

lazy_static! {
  static ref CSS_RE: Regex = Regex::new(r"(?U)((?:[a-zA-Z_]|--)[a-zA-Z_0-9-]*):\s*(.+)(?:;|;$|$)").unwrap();
}

impl CodegenContext <'_> {
  /// Generates attributes as a Js object,
  /// where keys are attribute names and values are attribute values.
  ///
  /// - `generate_obj_shell` is for surrounding the resulting object in the Js {} object notation
  pub fn generate_attributes(
    &mut self,
    buf: &mut String,
    attributes: &Vec<HtmlAttribute>,
    generate_obj_shell: bool,
    template_scope_id: u32
  ) -> bool {
    /* Work is not needed if we don't have any Regular attributes, v-on/v-bind directives */
    if !has_attributes_work(attributes) {
      return false;
    }

    // Start a Js object notation
    if generate_obj_shell {
      buf.push('{');
      self.code_helper.indent();
      self.code_helper.newline(buf);
    }

    /* For adding a comma `,` */
    let mut has_first_element = false;

    /* For `withModifiers` import */
    let mut has_used_modifiers_import = false;

    // Special generation for `class` and `style` attributes,
    // as they can have both Regular and VDirective variants
    let mut class_regular = None;
    let mut class_bind = None;
    let mut style_regular = None;
    let mut style_bind = None;

    for attribute in attributes {
      match attribute {
        // First, we check the special case: `class` and `style` attributes
        HtmlAttribute::Regular { name: "class", value } => {
          class_regular = Some(*value);
        },

        HtmlAttribute::VDirective(VDirective { name: "bind", argument: "class", value, .. }) => {
          class_bind = *value;
        },

        HtmlAttribute::Regular { name: "style", value } => {
          style_regular = Some(*value);
        },

        HtmlAttribute::VDirective(VDirective { name: "bind", argument: "style", value, .. }) => {
          style_bind = *value;
        },

        HtmlAttribute::Regular { name, value } => {
          if has_first_element {
            self.code_helper.comma_newline(buf);
          }

          /* For obvious reasons, attributes containing a dash need to be escaped */
          let needs_quotes = CodeHelper::needs_escape(name);

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

        // todo generate multiple attributes bound with v-bind, v-on
        HtmlAttribute::VDirective (VDirective { name, argument, modifiers, value, .. }) => {
          if has_first_element {
            self.code_helper.comma_newline(buf);
          }

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

    // Process `class` attribute. We may have a regular one, a bound one, both or neither.
    match (class_regular, class_bind) {
      (Some(regular_value), Some(bound_value)) => {
        if has_first_element {
          self.code_helper.comma_newline(buf);
        }

        buf.push_str("class: ");
        buf.push_str(self.get_and_add_import_str(VueImports::NormalizeClass));
        CodeHelper::open_paren(buf);
        CodeHelper::open_sq_bracket(buf);

        // First, include the content of a regular class
        CodeHelper::quoted(buf, regular_value);
        CodeHelper::comma(buf);

        let generated_class_binding = transform_scoped(
          bound_value,
          &self.scope_helper,
          template_scope_id
        ).unwrap_or_default();

        buf.push_str(&generated_class_binding);

        CodeHelper::close_sq_bracket(buf);
        CodeHelper::close_paren(buf);
      },

      (Some(regular_value), None) => {
        
      },

      (None, Some(bound_value)) => {

      },

      (None, None) => {}
    }

    // Close a Js object notation
    if generate_obj_shell {
      self.code_helper.unindent();
      self.code_helper.newline(buf);
      buf.push('}');
    }

    /* Add imports */
    if has_used_modifiers_import {
      self.add_to_imports(VueImports::WithModifiers);
    }

    true
  }
}

/// Check if there is work regarding attributes generation
/// Work is not needed if we don't have any Regular attributes, v-on/v-bind directives
pub fn has_attributes_work(attributes: &Vec<HtmlAttribute>) -> bool {
  attributes
    .iter()
    .any(|it| match it {
      HtmlAttribute::Regular { .. } |
      HtmlAttribute::VDirective (VDirective { name: "on" | "bind", .. }) => true,
      _ => false
    })
}

fn generate_v_bind_attr(buf: &mut String, argument: &str, value: Option<&str>) {
  // todo what to do when you have the same regular attribute??
  // I don't want to do O(n^2) search for every attribute, but I also don't want to allocate any extra memory for a vec/set
  // maybe the analysis lib should report such issues??
  // current js SFC compiler just discards the second appearance of the same attribute name (except for class)

  /* Do not generate empty values */
  let Some(value_expression) = value else {
    // At this point js SFC compiler just panicks
    return
  };

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
    HtmlAttribute::VDirective (VDirective { name: "test", ..Default::default() }),
    HtmlAttribute::Regular { name: "class", value: "test" },
    HtmlAttribute::Regular { name: "readonly", value: "" },
    HtmlAttribute::Regular { name: "style", value: "background: red" },
    HtmlAttribute::Regular { name: "dashed-attr-name", value: "spaced attr value" }
  ];

  ctx.generate_attributes(&mut buf, &attributes, true, 0);
  assert_eq!(
    &buf,
    &format!(r#"{initial_buf}{{class: "test", readonly: "", style: "background: red", "dashed-attr-name": "spaced attr value"}}"#)
  );
}
