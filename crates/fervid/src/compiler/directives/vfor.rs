use fervid_core::{StartingTag, HtmlAttribute, VDirective};

use crate::{compiler::{codegen::CodegenContext, imports::VueImports, helper::CodeHelper}};

impl CodegenContext<'_> {
  /// Generates `(openBlock(true), createElementBlock(Fragment, null, renderList(<list>, (<item>) => { return`
  pub fn generate_vfor_prefix(
    &mut self,
    buf: &mut String,
    starting_tag: &StartingTag
  ) -> bool {
    let v_for = starting_tag
      .attributes
      .iter()
      .find_map(|attr| match attr {
        HtmlAttribute::VDirective(directive)
        if directive.name == "for" && directive.value.is_some() => Some(directive),

        _ => None
      });

    match v_for {
      Some(directive) => {
        // `(openBlock(true), `
        CodeHelper::open_paren(buf);
        buf.push_str(self.get_and_add_import_str(VueImports::OpenBlock));
        buf.push_str("(true), ");

        // `createElementBlock(Fragment, null, renderList(`
        buf.push_str(self.get_and_add_import_str(VueImports::CreateElementBlock));
        CodeHelper::open_paren(buf);
        buf.push_str(self.get_and_add_import_str(VueImports::Fragment));
        buf.push_str(", null, ");
        buf.push_str(self.get_and_add_import_str(VueImports::RenderList));
        CodeHelper::open_paren(buf);

        let (itervar, iterable) = split_itervar_and_iterable(
          directive.value.expect("directive value should be checked previously")
        );

        // Add iterable
        // TODO Contextual compile
        buf.push_str(iterable);
        CodeHelper::comma(buf);

        // Add iterator variables
        let needs_paren = !itervar.starts_with('(');
        if needs_paren {
          CodeHelper::open_paren(buf);
        }
        buf.push_str(itervar);
        if needs_paren {
          CodeHelper::close_paren(buf);
        }

        // Add arrow function with return
        // Here, I replaced `=> { return` to `=> (` because it's the same
        buf.push_str(" => (");
        self.code_helper.indent();
        self.code_helper.newline(buf);

        true
      },

      None => false
    }
  }

  /// Function to close the `v-for` code generation.
  /// It must be called after the target element/component has been generated
  /// and only if `generate_vfor_prefix` returned `true`.
  pub fn generate_vfor_suffix(&mut self, buf: &mut String, starting_tag: &StartingTag) {
    let has_key = starting_tag.attributes
      .iter()
      .any(|attr| match attr {
        HtmlAttribute::VDirective(VDirective { name: "bind", argument: "key", .. }) => true,
        _ => false
      });

    self.code_helper.unindent();
    self.code_helper.newline(buf);

    // TODO This can also be `)), 64 /* STABLE_FRAGMENT */))` when iterable is a number (`v-for="i in 3"`)

    if has_key {
      buf.push_str(")), 128 /* KEYED_FRAGMENT */))");
    } else {
      buf.push_str(")), 256 /* UNKEYED_FRAGMENT */))");
    }
  }
}

fn split_itervar_and_iterable<'a>(raw: &'a str) -> (&'a str, &'a str) {
  // Try guessing: `item in iterable`
  let mut split = raw.splitn(2, " in ");
  if let (Some(itervar), Some(iterable)) = (split.next(), split.next()) {
    return (itervar.trim(), iterable.trim());
  }

  // Do `item of iterable`
  let mut split = raw.splitn(2, " of ");
  if let (Some(itervar), Some(iterable)) = (split.next(), split.next()) {
    return (itervar.trim(), iterable.trim());
  }

  // Invalid `v-for` should be dropped by analyzer
  unreachable!();
}
