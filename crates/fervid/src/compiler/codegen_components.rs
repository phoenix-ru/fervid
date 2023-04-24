use std::fmt::Write;
use crate::{parser::{attributes::{HtmlAttribute, VDirective}, structs::{StartingTag, Node, ElementNode}}, compiler::directives::needs_directive_wrapper};

use super::{codegen::CodegenContext, imports::VueImports, codegen_attributes, helper::CodeHelper};

impl <'a> CodegenContext <'a> {
  pub fn create_component_vnode(
    &mut self,
    buf: &mut String,
    element_node: &ElementNode,
    wrap_in_block: bool
  ) {
    let ElementNode { starting_tag, children, template_scope } = element_node;

    // First goes the v-for prefix: the component needs to be surrounded in a new block
    let had_v_for = self.generate_vfor_prefix(buf, starting_tag);

    // Special generation: `_withDirectives` prefix
    let needs_directive = needs_directive_wrapper(starting_tag, true);
    if needs_directive {
      buf.push_str(self.get_and_add_import_str(VueImports::WithDirectives));
      CodeHelper::open_paren(buf);
    }

    // Special generation: (openBlock(), createBlock(
    let should_wrap_in_block = wrap_in_block || had_v_for;
    if should_wrap_in_block {
      self.generate_create_block(buf);
    } else {
      buf.push_str(self.get_and_add_import_str(VueImports::CreateVNode));
      CodeHelper::open_paren(buf);
    }

    self.add_to_components_and_write(buf, starting_tag.tag_name);

    // Code below goes from the rightmost argument to the leftmost,
    // so that we can determine how many params to pass

    // todo use attributes analysis to generate `, 8 /* PROPS */, ["prop1", "prop2"]`
    // these are the props which use js, e.g. `:prop1="testRef"`, but not `:prop="true"`
    // also, `@custom-ev="$emit()"` and `@custom-ev="ev => $emit(testRef)"` don't need this,
    // but `@custom-ev="$emit"` does. I need to understand why
    let needs_props_hint = false;
    let has_children_work = children.len() > 0;
    //  || {
    //   // todo this optimization needs to be done in separate run
    //   if let (1, Some(Node::TextNode(_))) = (children.len(), children.get(0)) {
    //     false
    //   } else {
    //     true
    //   }
    // };

    // `v-model`s are processed before attributes but they result in the same Js object
    let mut vmodels = get_vmodels(starting_tag).peekable();
    let has_vmodels_work = vmodels.peek().is_some();

    // Attributes work is regular attributes plus `v-on` and `v-bind` directives
    let has_attributes_work = codegen_attributes::has_attributes_work(
      starting_tag.attributes.iter()
    );

    // Early exit helper macro
    macro_rules! early_exit {
      () => {
        if wrap_in_block {
          CodeHelper::close_paren(buf);
        }
        CodeHelper::close_paren(buf);

        // Generate directives array if needed
        if needs_directive {
          CodeHelper::comma(buf);
          self.generate_directives(buf, starting_tag, true, *template_scope);
          CodeHelper::close_paren(buf);
        }

        // Close v-for if it was there
        if had_v_for {
          self.generate_vfor_suffix(buf, starting_tag);
        }

        return
      };
    }

    // Early exit: close function call
    if !has_attributes_work && !has_children_work && !needs_props_hint && !has_vmodels_work {
      early_exit!();
    }

    // Attributes (default to null)
    CodeHelper::comma(buf);
    if has_attributes_work || has_vmodels_work {
      // Open Js object
      self.code_helper.obj_open_paren(buf, true);

      // Generate `v-model`s code first
      if has_vmodels_work {
        self.generate_vmodels(buf, vmodels);
      }

      if has_attributes_work {
        // Divide from previously generated v-model code
        if has_vmodels_work {
          self.code_helper.comma_newline(buf);
        }

        self.generate_attributes(
          buf,
          starting_tag.attributes.iter(),
          false,
           *template_scope
        );
      }

      // Close Js object
      self.code_helper.obj_close_paren(buf, true);
    } else if has_children_work || needs_props_hint {
      CodeHelper::null(buf);
    }

    // Try to exit again
    if !has_children_work && !needs_props_hint {
      early_exit!();
    }

    // Children (default to null)
    CodeHelper::comma(buf);
    if has_children_work {
      self.generate_slots(buf, children, *template_scope);
    } else if needs_props_hint {
      CodeHelper::null(buf);
    }

    if needs_props_hint {
      // CodeHelper::comma(buf);
      todo!()
    }

    // Yes, this is not "early", but the cleanup code is handy
    early_exit!();
  }

  pub fn generate_components_string(&mut self, buf: &mut String) {
    if self.components.len() == 0 {
      return;
    }

    let resolve_fn_str = self.get_and_add_import_str(VueImports::ResolveComponent);

    // We need sorted entries for stable output.
    // Entries are sorted by Js identifier (second element of tuple in hashmap entry)
    let mut sorted_components: Vec<(&String, &String)> = self.components.iter().collect();
    sorted_components.sort_by(|a, b| a.1.cmp(b.1));

    // Key is a component as used in template, value is the assigned Js identifier
    for (index, (component_name, identifier)) in sorted_components.iter().enumerate() {
      if index > 0 {
        self.code_helper.newline(buf);
      }

      write!(buf, "const {} = {}(\"{}\")", identifier, resolve_fn_str, component_name)
        .expect("Could not construct components");
    }
  }

  /// Tries to write a component variable name to the buffer.
  /// First, it checks if we already have the component registered in the HashMap.
  /// If so, it will write the &str to buf and exit.
  /// Otherwise, it allocates a new String, writes to buf and moves ownership to HashMap.
  fn add_to_components_and_write(&mut self, buf: &mut String, tag_name: &str) {
    /* Check component existence and early exit */
    let existing_component_name = self.components.get(tag_name);
    if let Some(component_name) = existing_component_name {
      buf.push_str(component_name);
      return;
    }

    /* _component_ prefix plus tag name */
    let mut component_name = tag_name.replace('-', "_");
    component_name.insert_str(0, "_component_");

    /* Add to buf */
    buf.push_str(&component_name);

    /* Add to map */
    self.components.insert(tag_name.to_owned(), component_name);
  }

  /// Generates `(openBlock(), createBlock(`
  #[inline]
  fn generate_create_block(&mut self, buf: &mut String) {
    CodeHelper::open_paren(buf);
    buf.push_str(self.get_and_add_import_str(VueImports::OpenBlock));
    CodeHelper::open_paren(buf);
    CodeHelper::close_paren(buf);
    CodeHelper::comma(buf);
    buf.push_str(self.get_and_add_import_str(VueImports::CreateBlock));
    CodeHelper::open_paren(buf);
  }

  /// Double-pass slots code generation
  /// First pass generates named slots, while the second is for default slot
  fn generate_slots(&mut self, buf: &mut String, children: &[Node], scope_to_use: u32) {
    // A child is not from default slot if it is a `<template>` element,
    // which has `v-slot` with attribute which name is other than `default`.
    // Example: regular elements, text, `<template>` and `<template v-slot>` are from the default slot.
    // `<template v-slot:some-slot>` is not a default slot
    // TODO Move to common/core, because analyzer also uses it
    let is_from_default_slot = |node: &Node| match node {
      Node::ElementNode(ElementNode { starting_tag, .. }) => {
        if starting_tag.tag_name != "template" {
          return true;
        }

        // Slot is not default if its `v-slot` has an argument which is not "" or "default"
        // `v-slot` is default
        // `v-slot:default` is default
        // `v-slot:custom` is not default
        !starting_tag
          .attributes
          .iter()
          .any(|attr| match attr {
            HtmlAttribute::VDirective (VDirective { name, argument, .. }) => {
              *name == "slot" && *argument != "" && *argument != "default"
            },

            HtmlAttribute::Regular { .. } => false
          })
      },

      // explicit just in case I decide to change node types and forget about this place
      Node::DynamicExpression { .. } | Node::TextNode(_) | Node::CommentNode(_) => true
    };

    // Start a Js object
    self.code_helper.obj_open_paren(buf, true);

    // For commas and default slot generation
    let mut needs_slot_comma = false;
    // let mut processed_named_slots = 0;

    // Because optimizer has already sorted component's children based on slots, it is safe to partition
    let partition_point = children.partition_point(|it| !is_from_default_slot(&it));

    // First pass: named slots. Those are `<template>` elements with a defined slot name
    for template in &children[..partition_point] {
      if needs_slot_comma {
        self.code_helper.comma_newline(buf);
      }

      let Node::ElementNode(ElementNode { starting_tag, children, template_scope }) = template else {
        unreachable!("This should be impossible")
      };

      // Find needed attribute and generate the header (slot name + ctx)
      for attr in starting_tag.attributes.iter() {
        let HtmlAttribute::VDirective (VDirective { name: "slot", argument, value, is_dynamic_slot, .. }) = attr else {
          continue;
        };

        // For dynamic slots, generate a dynamic slot name `[_ctx.slotName]
        // todo support Scope (what if the slot name comes from the template scope??)
        if *is_dynamic_slot {
          buf.push_str("[_ctx.");
          buf.push_str(argument);
          buf.push(']');
        } else {
          CodeHelper::quoted(buf, argument);
        }

        // Context. Generates `: _withCtx(() =>` or `: _withCtx((ctx) =>`
        buf.push_str(": ");
        buf.push_str(self.get_and_add_import_str(VueImports::WithCtx));
        CodeHelper::open_paren(buf);
        CodeHelper::parens_option(buf, *value);
        buf.push_str(" => ");

        break
      }

      // Children
      let had_children_work = self.generate_element_children(
        buf,
        children,
        false,
        *template_scope
      );
      if !had_children_work {
        buf.push_str("[]");
      }

      // todo mode hint, e.g. `_: 2 /* Dynamic */`

      CodeHelper::close_paren(buf);

      needs_slot_comma = true;
      // processed_named_slots += 1;
    }

    // Second pass: default slot

    // TODO support `<template v-slot>` and `<template v-slot:default>` by processing them in the named slots??
    // Current SFC compiler panicks or tries to discard children not in `<template>` if it's present alongside normal elements
    // That means that you can't simply put children inside if you want to have `<template v-slot:default>`,
    // but you have to put everything inside of it.
    // I think the biggest issue here is `<template v-slot:default="props">`, this needs to be checked and analyzed

    let default_slot_children = &children[partition_point..];

    if default_slot_children.len() > 0 {
      if needs_slot_comma {
        self.code_helper.comma_newline(buf);
      }

      buf.push_str("default: ");
      buf.push_str(self.get_and_add_import_str(VueImports::WithCtx));
      buf.push_str("(() => ");
      self.generate_element_children(
        buf,
        default_slot_children,
        false,
        scope_to_use
      );
      CodeHelper::close_paren(buf);
    }

    // End a Js object
    self.code_helper.obj_close_paren(buf, true);
  }

  fn generate_vmodels<'d>(&mut self, buf: &mut String, directives: impl Iterator<Item = &'d VDirective<'d>>) {
    for (index, directive) in directives.enumerate() {
      // todo throw away garbage values in v-model during analyzer pass (e.g. v-model="foo - bar" or v-model="")
      // yes, it will break commas when there are 2 v-models and the first is discarded
      let directive_value = directive.value.unwrap_or("");
      if directive_value == "" {
        continue;
      }

      if index != 0 {
        self.code_helper.comma_newline(buf)
      }

      // Prep: check if the directive arg needs quoting. If yes, it will be quoted everywhere
      let argument = if directive.argument.len() > 0 { directive.argument } else { "modelValue" };
      let needs_quoting = CodeHelper::needs_escape(argument);

      // First, generate the bound prop
      if needs_quoting {
        CodeHelper::quoted(buf, argument)
      } else {
        buf.push_str(argument)
      }
      CodeHelper::colon(buf);
      // todo context-aware codegen (scope checking)
      buf.push_str("_ctx.");
      buf.push_str(directive_value);
      self.code_helper.comma_newline(buf);

      // Second, generate "onUpdate:modelValue" or "onUpdate:usersArgument"
      CodeHelper::quote(buf);
      buf.push_str("onUpdate:");
      CodeHelper::to_camelcase(buf, argument);
      CodeHelper::quote(buf);
      buf.push_str(": ");

      // Third, generate a handler for onUpdate
      // For example, `$event => ((_ctx.modelValue) = $event)`
      // todo generate cache around it
      // todo context
      write!(buf, "$event => ((_ctx.{}) = $event)", directive_value).expect("writing to buf failed");

      // Optionally generate modifiers. Early exit if no work is needed
      if directive.modifiers.len() == 0 { continue; }

      // This is weird, but that's how the official compiler is implemented
      // modelValue => modelModifiers
      // users-argument => "users-argumentModifiers"
      self.code_helper.comma_newline(buf);
      if argument == "modelValue" {
        buf.push_str("model");
      } else {
        if needs_quoting {
          CodeHelper::quote(buf)
        }
        buf.push_str(argument)
      }
      buf.push_str("Modifiers");
      if needs_quoting {
        CodeHelper::quote(buf)
      }
      CodeHelper::colon(buf);

      let modifiers_iter = directive.modifiers.iter().map(|modifier| (*modifier, "true"));
      self.code_helper.obj_from_entries_iter(buf, modifiers_iter);
    }
  }
}

/// Gets all the v-model's of a tag
fn get_vmodels<'a>(starting_tag: &'a StartingTag) -> impl Iterator<Item = &'a VDirective<'a>> {
  starting_tag.attributes
    .iter()
    .filter_map(|attr| match attr {
        HtmlAttribute::VDirective(directive) if directive.name == "model" => Some(directive),
        _ => None
    })
}
