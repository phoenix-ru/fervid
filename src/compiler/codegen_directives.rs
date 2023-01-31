use std::fmt::Write;
use crate::parser::{structs::StartingTag, attributes::HtmlAttribute};

use super::{codegen::CodegenContext, helper::CodeHelper, imports::VueImports};

impl<'a> CodegenContext<'a> {
  pub fn generate_directives(&mut self, buf: &mut String, starting_tag: &'a StartingTag, is_component: bool) {
    // Open Js array
    CodeHelper::open_sq_bracket(buf);

    for attr in &starting_tag.attributes {
      if let HtmlAttribute::VDirective { name, argument, modifiers, value, is_dynamic_slot } = attr {
        // Do not process "bind", "on" and "slot"
        // Do not process "model" for `is_component`
        if *name == "bind" || *name == "on" || *name == "slot" || (is_component && *name == "model") {
          continue;
        }

        self.code_helper.indent();
        self.code_helper.newline(buf);

        // todo generate

        // Whether to split generated input across multiple lines or inline in one
        let has_argument = argument.len() > 0;
        let has_modifiers = modifiers.len() > 0;
        let has_arg_or_modifiers = has_argument || has_modifiers;

        // A directive is an array of
        // [<directive_ident>, <directive_value>?, <directive_arg>?, <directive_modifiers>?]
        CodeHelper::open_sq_bracket(buf);
        if has_arg_or_modifiers {
          self.code_helper.indent();
          self.code_helper.newline(buf);
        }

        // Write <directive_ident>. This is either from Vue (vModel*) or the identifier of custom directive
        if *name == "model" {
          let vmodel_directive = self.get_vmodel_directive_name(starting_tag);
          buf.push_str(vmodel_directive);
        } else {
          self.add_to_directives_and_write(buf, name);
        }

        // <directive_value>?
        if let Some(directive_value) = *value {
          if has_arg_or_modifiers {
            self.code_helper.comma_newline(buf);
          } else {
            CodeHelper::comma(buf);
          }

          // TODO use context-dependent variables (not msg, but _ctx.msg or $setup.msg)
          buf.push_str("_ctx.");
          buf.push_str(directive_value)
        } else if has_arg_or_modifiers {
          self.code_helper.comma_newline(buf);
          buf.push_str("void 0")
        }

        // <directive_arg>?
        if has_arg_or_modifiers {
          self.code_helper.comma_newline(buf);

          if has_argument {
            CodeHelper::quoted(buf, argument)
          } else {
            buf.push_str("void 0")
          }
        }

        // <directive_modifiers>?
        if has_modifiers {
          self.code_helper.comma_newline(buf);
          self.generate_modifiers_obj(buf, modifiers);
        }

        if has_arg_or_modifiers {
          self.code_helper.unindent();
          self.code_helper.newline(buf);
        }

        CodeHelper::close_sq_bracket(buf);
        self.code_helper.unindent();
      }
    }

    self.code_helper.newline(buf);

    CodeHelper::close_sq_bracket(buf);
  }

  /// Function for determining whether a given element/component
  /// needs to be wrapped in `_withDirectives(<node code>, <directives code>)`
  /// Typically, it depends on `is_component` flag:
  /// 1. `is_component = true` and has any directive except for 'on', 'bind', 'slot' and 'model';
  /// 2. `is_component = false` and has any directive except for 'on', 'bind' and 'slot'.

  pub fn needs_directive_wrapper(starting_tag: &StartingTag, is_component: bool) -> bool {
    let mut needs_vmodel = false;
    let mut needs_custom_directive = false;

    for attr in &starting_tag.attributes {
      match attr {
        HtmlAttribute::VDirective { name, .. } => {
          if *name == "model" {
            needs_vmodel = true;
          } else if *name != "bind" && *name != "on" && *name != "slot" {
            needs_custom_directive = true;
          }
        },

        _ => {}
      }
    }

    if is_component {
      needs_custom_directive
    } else {
      needs_vmodel || needs_custom_directive
    }
  }

  pub fn generate_directive_resolves(&mut self, buf: &mut String) {
    if self.directives.len() == 0 {
      return;
    }

    let resolve_fn_str = self.get_and_add_import_str(VueImports::ResolveDirective);

    // Key is a component as used in template, value is the assigned Js identifier
    for (index, (directive_name, identifier)) in self.directives.iter().enumerate() {
      if index > 0 {
        self.code_helper.newline(buf);
      }

      write!(buf, "const {} = {}(\"{}\")", identifier, resolve_fn_str, directive_name)
        .expect("Could not construct directives");
    }
  }

  fn add_to_directives_and_write(&mut self, buf: &mut String, directive_name: &'a str) {
    // Check directive existence and early exit
    let existing_directive_name = self.directives.get(directive_name);
    if let Some(directive_name) = existing_directive_name {
      buf.push_str(directive_name);
      return;
    }

    // _directive_ prefix plus directive name
    let mut directive_ident = directive_name.replace('-', "_");
    directive_ident.insert_str(0, "_directive_");

    // Add to buf
    buf.push_str(&directive_ident);

    // Add to map
    self.directives.insert(directive_name, directive_ident);
  }

  fn get_vmodel_directive_name(&mut self, starting_tag: &'a StartingTag) -> &'a str {
    // These cases need special handling of v-model
    // input type=* -> vModelText
    // input type="radio" -> vModelRadio
    // input type="checkbox" -> vModelCheckbox
    // select -> vModelSelect
    // textarea -> vModelText
    match starting_tag.tag_name {
      "input" => {
        let input_type = starting_tag.attributes
          .iter()
          .find_map(|input_attr| {
            match input_attr {
              HtmlAttribute::Regular { name: "type", value } => Some(*value),
              _ => None
            }
          })
          .unwrap_or("text");

        match input_type {
          "checkbox" => return self.get_and_add_import_str(VueImports::VModelCheckbox),
          "radio" => return self.get_and_add_import_str(VueImports::VModelRadio),
          _ => return self.get_and_add_import_str(VueImports::VModelText)
        }
      },

      "textarea" => return self.get_and_add_import_str(VueImports::VModelText),

      "select" => return self.get_and_add_import_str(VueImports::VModelSelect),

      _ => unreachable!("Adding v-model on native elements is only supported for <input>, <select> and <textarea>")
    }
  }

  /// Generates a Js object, where keys are modifier names and values are `true`
  /// For example, `v-directive:prop.foo.bar` would have `{ foo: true, bar: true }`
  fn generate_modifiers_obj(&mut self, buf: &mut String, modifiers: &[&str]) {
    buf.push('{');

    let is_multiline = modifiers.len() > 1;
    if is_multiline {
      self.code_helper.indent();
      self.code_helper.newline(buf);
    }

    for (index, modifier) in modifiers.iter().enumerate() {
      if index > 0 && is_multiline {
        self.code_helper.comma_newline(buf);
      } else if index > 0 {
        CodeHelper::comma(buf);
      }

      let needs_escape = modifier
        .chars()
        .enumerate()
        .any(|(c_index, c)| {
          // Unescaped Js idents must not start with a number and must be ascii alphanumeric
          (c_index == 0 && !c.is_ascii_alphabetic()) || (c_index > 0 && !c.is_ascii_alphanumeric())
        });

      if needs_escape {
        CodeHelper::quoted(buf, modifier)
      } else {
        buf.push_str(modifier)
      }

      CodeHelper::colon(buf);
      buf.push_str("true");
    }

    if is_multiline {
      self.code_helper.unindent();
      self.code_helper.newline(buf);
    }

    buf.push('}')
  }
}
