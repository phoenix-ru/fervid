use fervid_core::{ElementCodegenValue, ElementKind, Node, WithMemoCall};

use crate::{
    TransformSfcContext, template::node_transforms::TransformNodeState, utils::convert_to_block,
};

pub fn pre_transform_memo(
    ctx: &mut TransformSfcContext,
    state: &mut TransformNodeState,
    node: &mut Node,
) {
    // Optimization: checking `in_ssr` first is cheaper
    if ctx.in_ssr {
        return;
    }

    let Node::Element(element_node) = node else {
        return;
    };

    if element_node
        .starting_tag
        .directives
        .as_ref()
        .is_none_or(|v| v.v_memo.is_none())
    {
        return;
    };

    state.v_memo = true;
}

pub fn post_transform_memo(
    ctx: &mut TransformSfcContext,
    state: &TransformNodeState,
    node: &mut Node,
) {
    if !state.v_memo {
        return;
    }

    let Node::Element(element_node) = node else {
        return;
    };

    let Some(v_memo) = element_node
        .starting_tag
        .directives
        .as_ref()
        .and_then(|v| v.v_memo.as_ref())
    else {
        // This should not happen normally as `state.v_memo` guards the post hook.
        // Reaching this branch means that something either incorrectly set `state.v_memo = true`,
        // cleared directives, or took `v_memo`, all of which are unexpected.
        debug_assert!(
            false,
            "v_memo unset during post_transform_memo. directives.is_some() = {}",
            element_node.starting_tag.directives.is_some()
        );
        return;
    };

    let Some(mut codegen_node) = element_node
        .codegen_node
        .take_if(|v| matches!(v.value, ElementCodegenValue::VNodeCall(_)))
    else {
        return;
    };

    let ElementCodegenValue::VNodeCall(mut vnode_call) = codegen_node.value else {
        // Type was checked above
        unreachable!();
    };

    // Non-component subtree should be turned into a block
    if !(matches!(
        element_node.tag_type,
        ElementKind::Component | fervid_core::ElementKind::Builtin(_)
    ) && !matches!(
        element_node.tag_type,
        ElementKind::Builtin(fervid_core::BuiltinType::Slot)
    )) {
        convert_to_block(ctx, &mut vnode_call);
    }

    codegen_node.value = ElementCodegenValue::WithMemoCall(Box::new(WithMemoCall {
        expr: v_memo.to_owned(),
        vnode_call: *vnode_call,
    }));

    element_node.codegen_node = Some(codegen_node);
}
