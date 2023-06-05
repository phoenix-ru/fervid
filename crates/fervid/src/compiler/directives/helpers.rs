use fervid_core::{HtmlAttribute, StartingTag, VDirective};

/// Function for determining whether a given element/component
/// needs to be wrapped in `_withDirectives(<node code>, <directives code>)`
/// Typically, it depends on `is_component` flag:
/// 1. `is_component = true` and component has any directive other than 'on', 'bind', 'slot' and 'model';
/// 2. `is_component = false` and element has any directive other than 'on', 'bind' and 'slot'.

pub fn needs_directive_wrapper(starting_tag: &StartingTag, is_component: bool) -> bool {
    starting_tag.attributes.iter().any(|attr| match attr {
        HtmlAttribute::VDirective(directive) => match directive {
            VDirective::Model(_) if !is_component => true,
            VDirective::Custom(_) => true,
            VDirective::Show(_) => true,
            _ => false
        },

        _ => false,
    })
}
