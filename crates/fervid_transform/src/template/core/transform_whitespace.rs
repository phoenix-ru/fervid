use fervid_core::{ElementKind, Node, is_from_default_slot};

pub fn transform_whitespace(children: &mut Vec<Node>, element_kind: ElementKind) {
    let children_len = children.len();

    // Discard children mask, limited to 128 children. 0 means to preserve the node, 1 to discard
    let mut discard_mask: u128 = 0;

    // Filter out whitespace text nodes at the beginning and end of ElementNode
    match children.first() {
        Some(Node::Text(v, _)) if v.trim().is_empty() => {
            discard_mask |= 1 << 0;
        }
        _ => {}
    }
    match children.last() {
        Some(Node::Text(v, _)) if v.trim().is_empty() => {
            discard_mask |= 1 << (children_len - 1);
        }
        _ => {}
    }

    // For removing the middle whitespace text nodes, we need sliding windows of three nodes
    for (index, window) in children.windows(3).enumerate() {
        match window {
            [
                Node::Element(_) | Node::Comment(_, _),
                Node::Text(middle, _),
                Node::Element(_) | Node::Comment(_, _),
            ] if middle.trim().is_empty() => {
                discard_mask |= 1 << (index + 1);
            }
            _ => {}
        }
    }

    // Retain based on discard_mask. If a discard bit at `index` is set to 1, the node will be dropped
    let mut index = 0;
    children.retain(|_| {
        let should_retain = discard_mask & (1 << index) == 0;
        index += 1;
        should_retain
    });

    // For components, reorder children so that named slots come first
    if matches!(element_kind, ElementKind::Component) && !children.is_empty() {
        children.sort_by(|a, b| {
            let a_is_from_default = is_from_default_slot(a);
            let b_is_from_default = is_from_default_slot(b);

            a_is_from_default.cmp(&b_is_from_default)
        });
    }
}
