use std::fmt::Write;
use crate::parser::{structs::StartingTag, attributes::{HtmlAttribute, VDirective}};

use super::{codegen::CodegenContext, helper::CodeHelper, imports::VueImports};

impl<'a> CodegenContext<'a> {
  pub fn generate_directives(&mut self, buf: &mut String, starting_tag: &StartingTag, is_component: bool) {
    // Open Js array
    CodeHelper::open_sq_bracket(buf);

    for attr in &starting_tag.attributes {
      let HtmlAttribute::VDirective (VDirective {
        name,
        argument,
        modifiers,
        value,
        is_dynamic_slot: _
      }) = attr else { continue };

      if !supports_with_directive(*name, is_component) {
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
      // TODO better handle `is_component`
      if *name == "model" && !is_component {
        let vmodel_directive = self.get_vmodel_directive_name(starting_tag);
        buf.push_str(vmodel_directive);
      } else if *name == "show" {
        // v-show comes from "vue" import
        buf.push_str(self.get_and_add_import_str(VueImports::VShow));
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

        // Generates a Js object, where keys are modifier names and values are `true`
        // For example, `v-directive:prop.foo.bar` would have `{ foo: true, bar: true }`
        self.code_helper.obj_from_entries_iter(
          buf,
          modifiers.iter().map(|modifier| (*modifier, "true"))
        );
      }

      if has_arg_or_modifiers {
        self.code_helper.unindent();
        self.code_helper.newline(buf);
      }

      CodeHelper::close_sq_bracket(buf);
      self.code_helper.unindent();
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
    starting_tag.attributes.iter().any(|attr| {
      match attr {
        HtmlAttribute::VDirective (VDirective { name, .. }) => {
          supports_with_directive(*name, is_component)
        },

        _ => false
      }
    })
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

  fn add_to_directives_and_write(&mut self, buf: &mut String, directive_name: &str) {
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
    self.directives.insert(directive_name.to_owned(), directive_ident);
  }

  fn get_vmodel_directive_name(&mut self, starting_tag: &StartingTag) -> &'a str {
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
}

/// Checks if `withDirective` can be generated for a given directive name
/// "bind", "on" and "slot" are generated separately
/// "model" for `is_component` also has a separate logic
fn supports_with_directive(directive_name: &str, is_component: bool) -> bool {
  match directive_name {
    "bind" | "on" | "slot" | "if" | "else-if" | "else" | "for" => false,
    "model" if is_component => false,
    _ => true
  }
}
