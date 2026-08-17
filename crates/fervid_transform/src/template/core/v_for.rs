use fervid_core::{
    AttributeOrBinding, CacheMarker, ElementCodegenValue, ElementKind, ForCodegenNode, ForNode,
    ForParseResult, Node, PatchFlags, StrOrExpr, VForDirective,
};
use smallvec::SmallVec;
use swc_core::ecma::ast::{Expr, Lit, Str};

use crate::{
    TransformSfcContext,
    template::{collect_vars::collect_variables, expr_transform::BindingsHelperTransform},
};

pub fn pre_transform_for(ctx: &mut TransformSfcContext, node: &mut Node) {
    let Node::Element(element_node) = node else {
        return;
    };

    let Some(directives) = element_node.starting_tag.directives.as_mut() else {
        return;
    };

    // Structural directive transforms are not concerned with slots
    // as they are handled separately in vSlot.ts
    // https://github.com/vuejs/core/blob/b5f8518379b77c3b62a7a9d2b52f6c76cda09bd5/packages/compiler-core/src/transform.ts#L499-L503
    if matches!(element_node.tag_type, ElementKind::Template) && directives.v_slot.is_some() {
        return;
    }

    let Some(v_for) = directives.v_for.take() else {
        return;
    };

    process_for(ctx, node, v_for);
}

pub fn post_transform_for(ctx: &mut TransformSfcContext, node: &mut Node) {
    let Node::For(for_node) = node else {
        return;
    };

    ctx.directive_scopes.v_for -= 1;

    // TODO: Finish renderList codegen finalization. This currently only reconciles the child
    // vnode's block and patch requirements after its element transform has run
    let is_stable = for_node
        .codegen_node
        .as_deref()
        .expect("process_for should initialize ForNode codegen metadata")
        .patch_flags
        .contains(PatchFlags::StableFragment);

    let [Node::Element(element)] = for_node.children.as_mut_slice() else {
        return;
    };
    let Some(codegen_node) = element.codegen_node.as_mut() else {
        return;
    };
    let ElementCodegenValue::VNodeCall(vnode_call) = &mut codegen_node.value;

    let should_use_block = !is_stable || vnode_call.is_block_required;
    vnode_call.is_block = should_use_block;

    if !should_use_block && vnode_call.needs_patch {
        vnode_call.patch_hints.flags |= PatchFlags::NeedPatch;
    }
}

pub fn process_for(ctx: &mut TransformSfcContext, node: &mut Node, mut v_for: VForDirective) {
    let parent_scope = ctx.current_template_scope;

    let Node::Element(element_node) = node else {
        return;
    };
    let is_template = matches!(element_node.tag_type, ElementKind::Template);
    // transformOnce already entered its scope before transformFor replaces this element
    let has_v_once = element_node
        .starting_tag
        .directives
        .as_mut()
        .and_then(|directives| directives.v_once.take())
        .is_some();

    // TODO: Move to codegen
    let (has_key, mut key) = find_for_key(element_node);

    finalize_for_parse_result(ctx, &mut v_for.parse_result, parent_scope);

    let is_stable = matches!(v_for.parse_result.source.as_ref(), Expr::Lit(_));
    let patch_flags = if is_stable {
        PatchFlags::StableFragment.into()
    } else if has_key {
        PatchFlags::KeyedFragment.into()
    } else {
        PatchFlags::UnkeyedFragment.into()
    };
    // END TODO

    // Create new template scope
    let scope_to_use = ctx.bindings_helper.template_scopes.len() as u32;
    ctx.bindings_helper
        .template_scopes
        .push(crate::TemplateScope {
            variables: SmallVec::new(),
            parent: parent_scope,
        });

    // Replace original node with ForNode
    let mut original_node = std::mem::replace(
        node,
        Node::For(ForNode {
            parse_result: v_for.parse_result,
            children: vec![],
            template_scope: scope_to_use,
            // TODO Assign during codegen phase instead
            codegen_node: Some(Box::new(ForCodegenNode {
                patch_flags,
                disable_tracking: !is_stable,
                is_template,
                key: None,
                cache: if has_v_once {
                    CacheMarker::InVOnce.into()
                } else {
                    Default::default()
                },
            })),
            span: v_for.span,
        }),
    );

    let Node::For(for_node) = node else {
        // SAFETY - We just replaced the node above with ForNode
        unreachable!()
    };

    // Push either children when it is `<template v-for>` or the original Node itself
    if let Node::Element(ref mut element_node) = original_node
        && matches!(element_node.tag_type, ElementKind::Template)
    {
        let children = std::mem::take(&mut element_node.children);
        for_node.children = children;
    } else {
        for_node.children.push(original_node);
    }

    ctx.directive_scopes.v_for += 1;

    // Get the iterator variables and collect their variables
    let scope = &mut ctx.bindings_helper.template_scopes[scope_to_use as usize];
    collect_variables(&for_node.parse_result.value, scope);
    if let Some(key) = &for_node.parse_result.key {
        collect_variables(key, scope);
    }
    if let Some(index) = &for_node.parse_result.index {
        collect_variables(index, scope);
    }

    // TODO: Move to codegen
    if is_template && let Some(key) = key.as_mut() {
        ctx.bindings_helper.transform_expr(key, scope_to_use);
    }
    for_node
        .codegen_node
        .as_mut()
        .expect("process_for should initialize ForNode codegenNode")
        .key = key;

    // TODO: Re-check if this is implemented during codegen
    // // Add patch flags
    // if !is_dynamic {
    //     // This is `64 /* STABLE_FRAGMENT */`
    //     // when iterable is non-dynamic (number, string) (`v-for="i in 3"`)
    //     v_for.patch_flags |= PatchFlags::StableFragment;
    // } else {
    //     // Look for `key`. Fragment is either keyed or unkeyed.
    //     let has_key = element_node
    //         .starting_tag
    //         .attributes
    //         .iter()
    //         .any(|attr| check_attribute_name(attr, "key"));

    //     v_for.patch_flags |= if has_key {
    //         PatchFlags::KeyedFragment
    //     } else {
    //         PatchFlags::UnkeyedFragment
    //     };
    // }
}

fn find_for_key(node: &fervid_core::ElementNode) -> (bool, Option<Box<Expr>>) {
    for attribute in &node.starting_tag.attributes {
        match attribute {
            AttributeOrBinding::RegularAttribute { name, value, span } if name == "key" => {
                let key = (!value.is_empty()).then(|| {
                    Box::new(Expr::Lit(Lit::Str(Str {
                        span: *span,
                        value: value.clone(),
                        raw: None,
                    })))
                });
                return (true, key);
            }
            AttributeOrBinding::VBind(v_bind) if matches!(v_bind.argument.as_ref(), Some(StrOrExpr::Str(name)) if name.value == "key") =>
            {
                return (true, Some(v_bind.value.clone()));
            }
            AttributeOrBinding::VBind(v_bind) if matches!(v_bind.argument.as_ref(), Some(StrOrExpr::Expr(expr)) if matches!(expr.as_ref(), Expr::Lit(Lit::Str(name)) if name.value == "key")) =>
            {
                return (true, Some(v_bind.value.clone()));
            }
            _ => {}
        }
    }

    (false, None)
}

pub fn finalize_for_parse_result(
    ctx: &mut TransformSfcContext,
    result: &mut ForParseResult,
    scope_to_use: u32,
) {
    if result.finalized {
        return;
    }

    // TODO What is better here - has_scope_ref or just has_js_bindings
    result.finalized_is_dynamic = ctx
        .bindings_helper
        .transform_expr(&mut result.source, scope_to_use)
        .has_js_bindings;

    result.finalized = true;
}
