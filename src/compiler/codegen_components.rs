use std::fmt::Write;
use crate::parser::{attributes::HtmlAttribute, structs::{StartingTag, Node, ElementNode}};

use super::{codegen::CodegenContext, imports::VueImports, codegen_attributes, helper::CodeHelper};

impl <'a> CodegenContext <'a> {
  pub fn create_component_vnode(self: &mut Self, buf: &mut String, starting_tag: &'a StartingTag, children: &'a [Node]) {
    buf.push_str(self.get_and_add_import_str(VueImports::CreateVNode));
    buf.push('(');

    self.add_to_components_and_write(buf, starting_tag.tag_name);

    // Code below goes from the rightmost argument to the leftmost,
    // so that we can determine how many params to pass

    // todo use attributes analysis to generate `, 8 /* PROPS */, ["prop1", "prop2"]`
    // these are the props which use js, e.g. `:prop1="testRef"`, but not `:prop="true"`
    // also, `@custom-ev="$emit()"` and `@custom-ev="ev => $emit(testRef)"` don't need this,
    // but `@custom-ev="$emit"` does. I need to understand why
    let needs_props_hint = false;
    let has_children_work = children.len() > 0 || {
      // todo this optimization needs to be done in separate run
      if let (1, Some(Node::TextNode(_))) = (children.len(), children.get(0)) {
        false
      } else {
        true
      }
    };
    let has_attributes_work = codegen_attributes::has_attributes_work(&starting_tag.attributes);

    // Early exit: close function call
    if !has_attributes_work && !has_children_work && !needs_props_hint {
      CodeHelper::close_paren(buf);
      return;
    }

    // Attributes (default to null)
    CodeHelper::comma(buf);
    if has_attributes_work {
      self.generate_attributes(buf, &starting_tag.attributes);
    } else if has_children_work || needs_props_hint {
      CodeHelper::null(buf);
    }

    // Try to exit again
    if !has_children_work && !needs_props_hint {
      CodeHelper::close_paren(buf);
      return;
    }

    // Children (default to null)
    CodeHelper::comma(buf);
    if has_children_work {
      self.generate_slots(buf, children);
    } else if needs_props_hint {
      CodeHelper::null(buf);
    }

    if needs_props_hint {
      // CodeHelper::comma(buf);
      todo!()
    }

    CodeHelper::close_paren(buf)
  }

  pub fn generate_components_string(self: &mut Self, buf: &mut String) {
    if self.components.len() == 0 {
      return;
    }

    let resolve_fn_str = self.get_and_add_import_str(VueImports::ResolveComponent);

    // Key is a component as used in template, value is the assigned Js identifier
    for (index, (component_name, identifier)) in self.components.iter().enumerate() {
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
  fn add_to_components_and_write(self: &mut Self, buf: &mut String, tag_name: &'a str) {
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
    self.components.insert(tag_name, component_name);
  }

  /// Double-pass slots code generation
  /// First pass generates named slots, while the second is for default slot
  fn generate_slots(self: &mut Self, buf: &mut String, children: &'a [Node]) {
    // A child is not from default slot if it is a `<template>` element,
    // which has `v-slot` with attribute which name is other than `default`.
    // Example: regular elements, text, `<template>` and `<template v-slot>` are from the default slot.
    // `<template v-slot:some-slot>` is not a default slot
    let is_from_default_slot = |node: &&Node| match node {
      Node::ElementNode(ElementNode { starting_tag, .. }) => {
        starting_tag.tag_name != "template" || !starting_tag.attributes.iter().any(|attr| match attr {
          HtmlAttribute::VDirective { name, argument, .. } => {
            *name == "slot" && *argument != "" && *argument != "default"
          },
          HtmlAttribute::Regular { .. } => false
        })
      },

      // explicit just in case I decide to change node types and forget about this place
      Node::DynamicExpression(_) | Node::TextNode(_) | Node::CommentNode(_) => true
    };

    buf.push('{');

    // For commas and default slot generation
    let mut needs_slot_comma = false;
    // let mut processed_named_slots = 0;

    // First pass: named slots. Those are `<template>` elements with a defined slot name
    for template in children.iter().filter(|it| !is_from_default_slot(it)) {
      if needs_slot_comma {
        CodeHelper::comma(buf);
      }

      if let Node::ElementNode(ElementNode { starting_tag, children }) = template {
        // Find needed attribute and generate the header (slot name + ctx)
        for attr in starting_tag.attributes.iter() {
          if let HtmlAttribute::VDirective { name, argument, value, is_dynamic_slot, .. } = attr {
            if *name != "slot" {
              continue;
            }

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
            CodeHelper::colon(buf);
            buf.push_str(self.get_and_add_import_str(VueImports::WithCtx));
            CodeHelper::open_paren(buf);
            CodeHelper::parens_option(buf, *value);
            buf.push_str(" => ");

            break
          }
        }

        // Children
        let had_children_work = self.generate_element_children(buf, children, false);
        if !had_children_work {
          buf.push_str("[]");
        }

        // todo mode hint, e.g. `_: 2 /* Dynamic */`

        CodeHelper::close_paren(buf)
      } else {
        unreachable!("This should be impossible")
      }

      needs_slot_comma = true;
      // processed_named_slots += 1;
    }

    // Second pass: default slot
    // TODO I really don't like the idea of allocating a Vec just to call a function,
    // but I also don't understand how to pass iterators to functions
    let default_slot_children: Vec<&Node> = children.iter().filter(is_from_default_slot).collect();

    // TODO support `<template v-slot>` and `<template v-slot:default>` by processing them in the named slots??
    // Current SFC compiler panicks or tries to discard children not in `<template>` if it's present alongside normal elements
    // That means that you can't simply put children inside if you want to have `<template v-slot:default>`,
    // but you have to put everything inside of it.
    // I think the biggest issue here is `<template v-slot:default="props">`, this needs to be checked and analyzed

    if default_slot_children.len() > 0 {
      if needs_slot_comma {
        CodeHelper::comma(buf);
      }

      // TODO support ctx
      // TODO withCtx import
      buf.push_str("default: _withCtx(() => ");
      // TODO is passing `children` instead of `default_slot_children` a bug?
      self.generate_element_children(buf, children, false);
      CodeHelper::close_paren(buf);
    }

    buf.push('}')
  }
}
