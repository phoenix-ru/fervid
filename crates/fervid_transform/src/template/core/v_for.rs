use fervid_core::{ElementNode, PatchFlags, check_attribute_name};

use crate::{
    TransformSfcContext,
    template::{collect_vars::collect_variables, expr_transform::BindingsHelperTransform},
};

pub fn transform_for(
    ctx: &mut TransformSfcContext,
    node: &mut ElementNode,
    scope_to_use: u32,
    v_for_scope: &mut bool,
) {
    process_for(ctx, node, scope_to_use, v_for_scope);
}

pub fn process_for(
    ctx: &mut TransformSfcContext,
    node: &mut ElementNode,
    scope_to_use: u32,
    v_for_scope: &mut bool,
) {
    // Check if there is a scoping directive.
    // Find a `v-for` or `v-slot` directive when in ElementNode
    // and collect their variables into the new template scope
    // TODO(new-pipeline): move this scope tracking into Vue-aligned node transforms
    // (`trackVForSlotScopes` / `trackSlotScopes`) once transform-local exit state exists.
    if let Some(ref mut directives) = node.starting_tag.directives {
        let v_for = directives.v_for.as_mut();

        // Collect `v-for` bindings
        if let Some(v_for) = v_for {
            *v_for_scope = true;
            ctx.directive_scopes.v_for += 1;

            // Get the iterator variable and collect its variables
            let scope = &mut ctx.bindings_helper.template_scopes[scope_to_use as usize];
            collect_variables(&v_for.itervar, scope);

            // Transform the iterable
            let is_dynamic = ctx
                .bindings_helper
                .transform_expr(&mut v_for.iterable, scope_to_use);

            // Add patch flags
            if !is_dynamic {
                // This is `64 /* STABLE_FRAGMENT */`
                // when iterable is non-dynamic (number, string) (`v-for="i in 3"`)
                v_for.patch_flags |= PatchFlags::StableFragment;
            } else {
                // Look for `key`. Fragment is either keyed or unkeyed.
                let has_key = node
                    .starting_tag
                    .attributes
                    .iter()
                    .any(|attr| check_attribute_name(attr, "key"));

                v_for.patch_flags |= if has_key {
                    PatchFlags::KeyedFragment
                } else {
                    PatchFlags::UnkeyedFragment
                };
            }
        }
    }
}
