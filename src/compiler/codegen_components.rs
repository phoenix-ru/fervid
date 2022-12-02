use crate::parser::{StartingTag, Node};

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

      todo!("produce slots code...")

    } else if needs_props_hint {
      CodeHelper::null(buf);
    }

    if needs_props_hint {
      // CodeHelper::comma(buf);
      todo!()
    }

    CodeHelper::close_paren(buf)
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
}
