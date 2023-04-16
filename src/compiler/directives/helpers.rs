use crate::parser::{structs::StartingTag, attributes::{HtmlAttribute, VDirective}};

/// Function for determining whether a given element/component
/// needs to be wrapped in `_withDirectives(<node code>, <directives code>)`
/// Typically, it depends on `is_component` flag:
/// 1. `is_component = true` and component has any directive other than 'on', 'bind', 'slot' and 'model';
/// 2. `is_component = false` and element has any directive other than 'on', 'bind' and 'slot'.

pub fn needs_directive_wrapper(starting_tag: &StartingTag, is_component: bool) -> bool {
  starting_tag
    .attributes
    .iter()
    .any(|attr| {
      match attr {
        HtmlAttribute::VDirective (VDirective { name, .. }) => {
          supports_with_directive(*name, is_component)
        },

        _ => false
      }
    })
}

/// Checks if `withDirective` can be generated for a given directive name
/// "bind", "on" and "slot" are generated separately
/// "model" for `is_component` also has a separate logic
pub fn supports_with_directive(directive_name: &str, is_component: bool) -> bool {
  match directive_name {
    "bind" | "on" | "slot" | "if" | "else-if" | "else" | "for" => false,
    "model" if is_component => false,
    _ => true
  }
}
