use std::fmt::Write;

use fervid_core::{HtmlAttribute, VDirective, StartingTag, VCustomDirective, VModelDirective};

use super::{codegen::CodegenContext, helper::CodeHelper, imports::VueImports, transform::swc::transform_scoped};

impl<'a> CodegenContext<'a> {
  /// Generates directives array for `withDirectives` function call
  ///
  /// TODO This is a great target for experimental SWC compilation,
  /// although I will need to setup a writer interface just for emitting,
  /// which I kind of don't like
  pub fn generate_directives(
    &mut self,
    buf: &mut String,
    starting_tag: &StartingTag,
    is_component: bool,
    scope_to_use: u32
  ) {
    // Open Js array
    CodeHelper::open_sq_bracket(buf);

    // TODO Builtin directives are not handled here
    // TODO Generation for different built-in directives differs a lot
    // Do a more complex multi-step algorithm (e.g. `v-once` must be the last)
    // https://play.vuejs.org/#eNqdUktugzAUvMqrNyRSUtRtlEStuukNuihdEDDBqv2MwKapEHfvGEgU2i6iSgj7/WbGHnfiqaruWy/FRmybrFaVo0Y6X5FO+bhLhGsSsU9YmcrWjjqqZUE9FbU1FGEsSjjhzHLjyDRH2oX6InqRWlt6tbXO76Jlwtt4hAYQgolmTjBifJapk62sARSVCuDyNPDmski9Bn/CRHnq0sVy3BMIna/5HFGQ0WxolACAkOrDgh++uRInTaXBiIhoWz7su26Yp74nbC9q+n4bozp0Ka68o3ZtbC419KN/OABKuTfma52haBkVnCgRCA6Kc4QH6zkPmcdMq+wDmWkOk2ch6G60dZvpvFOZqFRTYzyXHE+UE79qAaCKcKu1lwO5kcYifns/R4HDcjYuQT9So04nTw4JcAmKgXhFJlbi2ZqfD2XuINHlkUC+YhkmLEt2vx7MX65yaiR8KwffVqHrBqcx8E+Pb/L0+gb6b717Dtg=

    for attr in &starting_tag.attributes {
      let HtmlAttribute::VDirective (directive) = attr else { continue };

      // Try to fit the old VDirective interface
      let empty_vec = vec![];
      let (name, argument, value, modifiers) = match directive {
        VDirective::Model(VModelDirective { argument, value, modifiers })
        if !is_component => (
          "model",
          *argument,
          Some(*value),
          modifiers
        ),

        VDirective::Show(condition) => ("show", None, Some(*condition), &empty_vec),

        VDirective::Custom(VCustomDirective { name, argument, modifiers, value }) => (
          *name,
          *argument,
          *value,
          modifiers
        ),

        _ => continue
      };

      self.code_helper.indent();
      self.code_helper.newline(buf);

      // todo generate

      // Whether to split generated input across multiple lines or inline in one
      let has_argument = argument.is_some();
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
      if name == "model" && !is_component {
        let vmodel_directive = self.get_vmodel_directive_name(starting_tag);
        buf.push_str(vmodel_directive);
      } else if name == "show" {
        // v-show comes from "vue" import
        buf.push_str(self.get_and_add_import_str(VueImports::VShow));
      } else {
        self.add_to_directives_and_write(buf, name);
      }

      // <directive_value>?
      if let Some(directive_value) = value {
        if has_arg_or_modifiers {
          self.code_helper.comma_newline(buf);
        } else {
          CodeHelper::comma(buf);
        }

        // Transform the directive value
        let transformed = transform_scoped(directive_value, &self.scope_helper, scope_to_use);
        buf.push_str(match transformed {
          Some(ref v) => &v,
          None => "void 0"
        });
      } else if has_arg_or_modifiers {
        self.code_helper.comma_newline(buf);
        buf.push_str("void 0")
      }

      // <directive_arg>?
      if has_arg_or_modifiers {
        self.code_helper.comma_newline(buf);

        if let Some(argument) = argument {
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

  pub fn generate_directive_resolves(&mut self, buf: &mut String) {
    if self.directives.len() == 0 {
      return;
    }

    let resolve_fn_str = self.get_and_add_import_str(VueImports::ResolveDirective);

    // We need sorted entries for stable output.
    // Entries are sorted by Js identifier (second element of tuple in hashmap entry)
    let mut sorted_directives: Vec<(&String, &String)> = self.directives.iter().collect();
    sorted_directives.sort_by(|a, b| a.1.cmp(b.1));

    // Key is a component as used in template, value is the assigned Js identifier
    for (index, (directive_name, identifier)) in sorted_directives.iter().enumerate() {
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
