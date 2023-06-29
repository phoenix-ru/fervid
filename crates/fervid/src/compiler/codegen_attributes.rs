use fervid_core::{HtmlAttribute, VDirective, VBindDirective, VOnDirective};
use lazy_static::lazy_static;
use regex::Regex;
use crate::analyzer::scope::ScopeHelper;
use super::codegen::CodegenContext;
use super::helper::CodeHelper;
use super::imports::VueImports;
use super::transform::swc::transform_scoped;

lazy_static! {
  static ref CSS_RE: Regex = Regex::new(r"(?U)([a-zA-Z_-][a-zA-Z_0-9-]*):\s*(.+)(?:;|$)").unwrap();
}

impl CodegenContext <'_> {
  /// Generates attributes as a Js object,
  /// where keys are attribute names and values are attribute values.
  ///
  /// - `generate_obj_shell` is for surrounding the resulting object in the Js {} object notation
  pub fn generate_attributes<'a>(
    &mut self,
    buf: &mut String,
    attributes: impl Iterator<Item = &'a HtmlAttribute<'a>> + Clone,
    generate_obj_shell: bool,
    template_scope_id: u32
  ) -> bool {
    /* Work is not needed if we don't have any Regular attributes, v-on/v-bind directives */
    if !has_attributes_work(attributes.clone()) {
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
        // class
        HtmlAttribute::Regular { name: "class", value } => {
          class_regular = Some(*value);
        },

        // :class
        HtmlAttribute::VDirective(VDirective::Bind(VBindDirective { argument: Some("class"), value, .. })) => {
          class_bind = Some(*value);
        },

        // style
        HtmlAttribute::Regular { name: "style", value } => {
          style_regular = Some(*value);
        },

        // :style
        HtmlAttribute::VDirective(VDirective::Bind(VBindDirective { argument: Some("style"), value, .. })) => {
          style_bind = Some(*value);
        },

        // Any regular attribute
        HtmlAttribute::Regular { name, value } => {
          if has_first_element {
            self.code_helper.comma_newline(buf);
          }

          // For obvious reasons, attributes containing a dash need to be escaped
          let needs_quotes = CodeHelper::needs_escape(name);

          // "attr-name": "attr value"
          if needs_quotes { buf.push('"'); }
          buf.push_str(name);
          if needs_quotes { buf.push('"'); }
          buf.push_str(": \"");
          buf.push_str(value);
          buf.push('"');

          has_first_element = true
        },

        // v-bind directive, shortcut `:`, e.g. `:custom-prop="value"`
        HtmlAttribute::VDirective(VDirective::Bind(v_bind)) => {
          if has_first_element {
            self.code_helper.comma_newline(buf);
          }

          generate_v_bind_attr(buf, v_bind, &self.scope_helper, template_scope_id);

          has_first_element = true
        }

        // v-on directive, shortcut `@`, e.g. `@custom-event.modifier="value"`
        HtmlAttribute::VDirective(VDirective::On(v_on)) => {
          if has_first_element {
            self.code_helper.comma_newline(buf);
          }

          generate_v_on_attr(buf, v_on, &self.scope_helper, template_scope_id);

          has_used_modifiers_import |= v_on.modifiers.len() > 0;
          has_first_element = true
        },

        _ => {}
      }
    }

    // Generate the attribute key if we have a `class` attr.
    if class_regular.is_some() || class_bind.is_some() {
      if has_first_element {
        self.code_helper.comma_newline(buf);
      }

      buf.push_str("class: ");

      has_first_element = true;
    }

    // Process `class` attribute. We may have a regular one, a bound one, both or neither.
    match (class_regular, class_bind) {
      // Both regular `class` and bound `:class`
      (Some(regular_value), Some(bound_value)) => {
        buf.push_str(self.get_and_add_import_str(VueImports::NormalizeClass));
        CodeHelper::open_paren(buf);
        CodeHelper::open_sq_bracket(buf);

        // First, include the content of a regular class
        CodeHelper::quoted(buf, regular_value);

        transform_scoped(
          bound_value,
          &self.scope_helper,
          template_scope_id
        ).map(|transformed| {
          CodeHelper::comma(buf);
          buf.push_str(&transformed);
        });

        CodeHelper::close_sq_bracket(buf);
        CodeHelper::close_paren(buf);
      },

      // Just regular `class`
      (Some(regular_value), None) => {
        CodeHelper::quoted(buf, regular_value);
      },

      // Just bound `:class`
      (None, Some(bound_value)) => {
        buf.push_str(self.get_and_add_import_str(VueImports::NormalizeClass));
        CodeHelper::open_paren(buf);

        transform_scoped(
          bound_value,
          &self.scope_helper,
        template_scope_id
        ).map(|transformed| {
          buf.push_str(&transformed);
        });

        CodeHelper::close_paren(buf);
      },

      // Neither
      (None, None) => {}
    }

    // Generate the `style` the same way as `class`
    if style_regular.is_some() || style_bind.is_some() {
      if has_first_element {
        self.code_helper.comma_newline(buf);
      }

      buf.push_str("style: ");
    }

    match (style_regular, style_bind) {
      // Both `style` and `:style`
      (Some(regular_value), Some(bound_value)) => {
        buf.push_str(self.get_and_add_import_str(VueImports::NormalizeStyle));
        CodeHelper::open_paren(buf);
        CodeHelper::open_sq_bracket(buf);

        // Regular style first
        generate_regular_style(buf, regular_value);

        // Then bound
        transform_scoped(
          bound_value,
          &self.scope_helper,
          template_scope_id
        ).map(|it| {
          CodeHelper::comma(buf);
          buf.push_str(&it)
        });

        CodeHelper::close_paren(buf);
        CodeHelper::close_sq_bracket(buf);
      },

      // `style`
      (Some(regular_value), None) => {
        generate_regular_style(buf, regular_value);
      },

      // `:style`
      (None, Some(bound_value)) => {
        buf.push_str(self.get_and_add_import_str(VueImports::NormalizeStyle));
        CodeHelper::open_paren(buf);

        transform_scoped(
          bound_value,
          &self.scope_helper,
          template_scope_id
        ).map(|it| {
          buf.push_str(&it)
        });

        CodeHelper::close_paren(buf);
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
pub fn has_attributes_work<'a>(mut attributes_iter: impl Iterator<Item = &'a HtmlAttribute<'a>>) -> bool {
  attributes_iter
    .any(|it| match it {
      HtmlAttribute::Regular { .. } |
      HtmlAttribute::VDirective (VDirective::Bind(_) | VDirective::On(_)) => true,
      _ => false
    })
}

fn generate_regular_style(buf: &mut String, style: &str) {
  let matches = CSS_RE
    .captures_iter(style)
    .filter_map(|mat| {
      let Some(style_name) = mat.get(1).map(|v| v.as_str().trim()) else { return None; };
      let Some(style_value) = mat.get(2).map(|v| v.as_str().trim()) else { return None; };

      Some((style_name, style_value))
    });

  CodeHelper::obj_from_entries_iter_inline(buf, matches, true);
}

fn generate_v_bind_attr(
  buf: &mut String,
  v_bind: &VBindDirective,
  scope_helper: &ScopeHelper,
  scope_to_use: u32
) {
  // todo what to do when you have the same regular attribute??
  // I don't want to do O(n^2) search for every attribute, but I also don't want to allocate any extra memory for a vec/set
  // maybe the analysis lib should report such issues??
  // current js SFC compiler just discards the second appearance of the same attribute name (except for class)

  // Do not generate empty values?
  let value_expression = v_bind.value;

  // IN:
  // v-on="ons" v-bind="bounds" @click=""
  //
  // OUT:
  // _mergeProps(_toHandlers(_ctx.ons), _ctx.bounds, {
  //   onClick: _cache[1] || (_cache[1] = () => {})
  // })
  let Some(argument) = v_bind.argument else {
    todo!("v-bind without argument is not implemented yet")
  };

  // For obvious reasons, attributes containing a dash need to be escaped
  let needs_quotes = argument.contains('-');

  // `"attr-name": `
  if needs_quotes {
    CodeHelper::quoted(buf, argument)
  } else {
    buf.push_str(argument)
  }
  buf.push_str(": ");

  // Transformed attr_expression
  // TODO Handle SWC failure
  let transform_result = transform_scoped(value_expression, scope_helper, scope_to_use);
  match transform_result {
    Some(transformed) => buf.push_str(&transformed),
    None => buf.push_str(r#""""#)
  }
}

/// Generates the code for a v-on listener
/// - buf is where to write the resulting code
/// - argument is the name of the event to listen to (e.g. `click` in `@click`)
/// - modifiers are event modifiers (e.g. `stop`, `prevent` in `@click.stop.prevent`)
/// - value is the value of the listener, typically an expression (e.g. `doSmth` in `@click="doSmth"`)
fn generate_v_on_attr(
  buf: &mut String,
  v_on: &VOnDirective,
  scope_helper: &ScopeHelper,
  scope_to_use: u32
) {
  // Transform name of event to camelCase, e.g. `onCustomEventName`
  buf.push_str("on");

  // IN:
  // v-on="ons" v-bind="bounds" @click=""
  //
  // OUT:
  // _mergeProps(_toHandlers(_ctx.ons), _ctx.bounds, {
  //   onClick: _cache[1] || (_cache[1] = () => {})
  // })
  let Some(event_name) = v_on.event else {
    todo!("v-on without argument is not implemented yet")
  };

  for word in event_name.split('-') {
    let first_char = word.chars().next();
    if let Some(ch) = first_char {
      // Uppercase the first char and append to buf
      for ch_component in ch.to_uppercase() {
        buf.push(ch_component);
      }

      // Push the rest of the word
      buf.push_str(&word[ch.len_utf8()..]);
    }
  }

  buf.push_str(": ");

  // Modifiers present: add function call
  if v_on.modifiers.len() > 0 {
    buf.push_str(CodegenContext::get_import_str(VueImports::WithModifiers));
    buf.push('(');
  }

  // Try compiling the expression
  // TODO Handle SWC failure
  let transformed_expr = v_on.handler
    .and_then(|expr| transform_scoped(expr, scope_helper, scope_to_use));

  // Value may be absent, e.g. `@click.stop`
  buf.push_str(match transformed_expr {
    Some(ref v) => v,
    None => "() => {}" // todo use _cache
  });

  // Modifiers present: add them in a Js array of strings
  if v_on.modifiers.len() > 0 {
    buf.push_str(", [");

    for (index, modifier) in v_on.modifiers.iter().enumerate() {
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

// #[test]
// fn test_attributes_generation() {
//   use fervid_core::VCustomDirective;

//   let initial_buf = "fn(arg1, ";
//   let mut buf = String::from(initial_buf);
//   let mut ctx: CodegenContext = Default::default();

//   let attributes = vec![
//     HtmlAttribute::VDirective (VDirective::Custom(VCustomDirective { name: "test", ..Default::default() })),
//     HtmlAttribute::Regular { name: "class", value: "test" },
//     HtmlAttribute::Regular { name: "readonly", value: "" },
//     HtmlAttribute::Regular { name: "style", value: "background: red" },
//     HtmlAttribute::Regular { name: "dashed-attr-name", value: "spaced attr value" }
//   ];

//   ctx.generate_attributes(&mut buf, attributes.iter(), true, 0);
//   assert_eq!(
//     &buf,
//     &format!(r#"{initial_buf}{{class: "test", readonly: "", style: "background: red", "dashed-attr-name": "spaced attr value"}}"#)
//   );
// }
