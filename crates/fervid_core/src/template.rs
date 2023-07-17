use crate::{Node, ElementNode};

/// Checks whether a Node is from the component's default slot or not
pub fn is_from_default_slot(node: &Node) -> bool {
    let Node::Element(ElementNode { starting_tag, .. }) = node else {
        return true;
    };

    if starting_tag.tag_name != "template" {
        return true;
    }

    // Slot is not default if its `v-slot` has an argument which is not "" or "default"
    // `v-slot` is default
    // `v-slot:default` is default
    // `v-slot:custom` is not default
    let Some(ref directives) = starting_tag.directives else { return true; };
    let Some(ref v_slot) = directives.v_slot else { return true; };
    if v_slot.is_dynamic_slot {
        return false;
    }

    match v_slot.slot_name {
        None | Some("default") => true,
        Some(_) => false,
    }
}
