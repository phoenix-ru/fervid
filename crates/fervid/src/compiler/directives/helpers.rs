use fervid_core::StartingTag;

/// Function for determining whether a given element/component
/// needs to be wrapped in `_withDirectives(<node code>, <directives code>)`
/// Typically, it depends on `is_component` flag:
/// 1. `is_component = true` and component has any directive other than 'on', 'bind', 'slot' and 'model';
/// 2. `is_component = false` and element has any directive other than 'on', 'bind' and 'slot'.

pub fn needs_directive_wrapper(starting_tag: &StartingTag, is_component: bool) -> bool {
    let Some(ref directives) = starting_tag.directives else {
        return false;
    };

    (!is_component && directives.v_model.len() != 0)
        || directives.v_show.is_some()
        || directives.custom.len() != 0
}
