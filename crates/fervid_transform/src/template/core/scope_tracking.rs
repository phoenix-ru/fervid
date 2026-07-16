use fervid_core::ElementNode;
use smallvec::SmallVec;

use crate::{
    TemplateScope, TransformSfcContext,
    template::core::{v_for::transform_for, v_slot::track_slot_scopes},
};

pub struct ElementScopeSnapshot {
    pub parent_scope: u32,
    pub old_ctx_scope: u32,
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
    let mut scope_to_use = parent_scope;

    let mut has_v_for = false;
    let mut has_v_slot = false;
    if let Some(directives) = &element_node.starting_tag.directives {
        has_v_for = directives.v_for.is_some();
        has_v_slot = directives.v_slot.is_some();
    }

    // Create a new scope
    if has_v_for || has_v_slot {
        // New scope will have ID equal to length
        scope_to_use = ctx.bindings_helper.template_scopes.len() as u32;
        ctx.bindings_helper.template_scopes.push(TemplateScope {
            variables: SmallVec::new(),
            parent: parent_scope,
        });
    }

    transform_for(ctx, element_node, scope_to_use, v_for_scope);

    // Collect `v-slot` bindings
    track_slot_scopes(ctx, element_node, scope_to_use);

    element_node.template_scope = scope_to_use;
    *current_scope = scope_to_use;
    ctx.current_template_scope = scope_to_use;

    snapshot
}

pub fn save_element_scope_snapshot(
    ctx: &mut TransformSfcContext,
    parent_scope: u32,
    v_for_scope: &mut bool,
) -> ElementScopeSnapshot {
    ElementScopeSnapshot {
        parent_scope,
        old_ctx_scope: ctx.current_template_scope,
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
    ctx.current_template_scope = snapshot.old_ctx_scope;
    *v_for_scope = snapshot.old_v_for_scope;
    ctx.directive_scopes.v_for = snapshot.old_directive_v_for;
    ctx.directive_scopes.v_slot = snapshot.old_directive_v_slot;
}
