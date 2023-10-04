use swc_core::ecma::atoms::js_word;

use crate::{ElementNode, Node, StrOrExpr};

/// Checks whether a Node is from the component's default slot or not
pub fn is_from_default_slot(node: &Node) -> bool {
    let Node::Element(ElementNode { starting_tag, .. }) = node else {
        // TODO: <template v-if="true" v-slot:foo>
        // https://play.vuejs.org/#eNp9UT1PwzAQ/SvWzW0YukWABKgDDICA0UuUXlIXx7Z85xCpyn/HdvqpVp3sex+n93RbeHKu6ANCCfeMndMV46M0QsSJeF7bzuUxAxMt+rlqHiSwDyghTqQtl421O6EQ8b/z3J3tPF+CmtJz4V6rq+Y0HhOdkDADptqaRrXFhqyJVbbJICFplUb/4VhZQxJKkZnEVVrbv7eMpSKzPV6vsf69gm9oSJiET4+Evo/VDxxXvkWe6OX3Ow7xfyA7uwo6qm+QX0hWh5Rxkj0Hs4qxT3Q57WvnrGdl2h9aDoyG9qXyJaJyzHoJ8Z4vN6of4y6KRfZJM8L4D55mqXA=
        // Node::ConditionalSeq(_) => true,

        return true;
    };

    if !starting_tag.tag_name.eq("template") {
        return true;
    }

    // Slot is not default if its `v-slot` has an argument which is not "" or "default"
    // `v-slot` is default
    // `v-slot:default` is default
    // `v-slot:custom` is not default
    let Some(ref directives) = starting_tag.directives else {
        return true;
    };
    let Some(ref v_slot) = directives.v_slot else {
        return true;
    };

    match v_slot.slot_name.as_ref() {
        None => true,
        Some(StrOrExpr::Str(js_word!("default"))) => true,
        Some(_) => false,
    }
}
