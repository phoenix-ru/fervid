use fervid_core::ElementNode;

use crate::TransformSfcContext;

pub struct ElementScopeSnapshot {
    pub parent_scope: u32,
    pub old_v_for_scope: bool,
    pub old_directive_v_for: u8,
    pub old_directive_v_slot: u8,
}

pub fn enter_element_scope(
    ctx: &mut TransformSfcContext,
    element_node: &mut ElementNode,
    current_scope: &mut u32,
    v_for_scope: &mut bool,
) -> ElementScopeSnapshot {
    let parent_scope = *current_scope;
    let snapshot = save_element_scope_snapshot(ctx, parent_scope, v_for_scope);
    let scope_to_use = parent_scope;

    element_node.template_scope = scope_to_use;
    *current_scope = scope_to_use;

    snapshot
}

pub fn save_element_scope_snapshot(
    ctx: &mut TransformSfcContext,
    parent_scope: u32,
    v_for_scope: &mut bool,
) -> ElementScopeSnapshot {
    ElementScopeSnapshot {
        parent_scope,
        old_v_for_scope: *v_for_scope,
        old_directive_v_for: ctx.directive_scopes.v_for,
        old_directive_v_slot: ctx.directive_scopes.v_slot,
    }
}

pub fn restore_element_scope_snapshot(
    ctx: &mut TransformSfcContext,
    snapshot: ElementScopeSnapshot,
    current_scope: &mut u32,
    v_for_scope: &mut bool,
) {
    *current_scope = snapshot.parent_scope;
    *v_for_scope = snapshot.old_v_for_scope;
    ctx.directive_scopes.v_for = snapshot.old_directive_v_for;
    ctx.directive_scopes.v_slot = snapshot.old_directive_v_slot;
}
