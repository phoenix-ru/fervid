use std::borrow::Cow;

use fervid_core::{
    AttributeOrBinding, CacheMarker, ElementCodegenValue, ElementKind, ElementNode, ForCodegenNode,
    ForNode, ForParseResult, Node, PatchFlags, PropsExpression, StrOrExpr, VForDirective,
};
use smallvec::SmallVec;
use swc_core::{
    common::Spanned,
    ecma::ast::{Expr, Lit, Str},
};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TransformError},
    template::{
        collect_vars::collect_variables,
        expr_transform::BindingsHelperTransform,
        node_transforms::TransformNodeState,
        utils::{find_prop, has_unnormalized_prop, inject_prop},
    },
};

pub fn pre_transform_for(
    ctx: &mut TransformSfcContext,
    state: &mut TransformNodeState,
    node: &mut Node,
) {
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

    process_for(ctx, state, node, v_for);
}

pub fn post_transform_for(
    ctx: &mut TransformSfcContext,
    state: &mut TransformNodeState,
    node: &mut Node,
) {
    let Node::For(for_node) = node else {
        return;
    };
    let Some(previous_current_scope) = state.for_scope else {
        return;
    };

    ctx.directive_scopes.v_for -= 1;
    state.current_scope = previous_current_scope;

    let Some(for_codegen_node) = for_node.codegen_node.as_mut() else {
        return;
    };

    // Check <template v-for> key placement
    if !ctx.bindings_helper.is_prod && for_codegen_node.is_template {
        for child in for_node.children.iter() {
            let Node::Element(element_node) = child else {
                continue;
            };

            if let Some(key) = find_prop(element_node, "key", false, false) {
                let span = match key {
                    AttributeOrBinding::RegularAttribute { span, .. } => *span,
                    AttributeOrBinding::VBind(v_bind) => v_bind.span,
                    AttributeOrBinding::VOn(_) => unreachable!("find_prop should not return VOn"),
                };
                ctx.errors
                    .push(TransformError::TemplateError(TemplateError {
                        span,
                        kind: crate::error::TemplateErrorKind::VForTemplateKeyPlacement,
                    }));
                break;
            }
        }
    }

    // v-memo processing optimization for `v-for v-memo` nodes.
    // vuejs-core does processing as follows:
    // - transformMemo(original_node)
    // -> seen.add(original_node)
    // -> v-for wraps original_node inside ForNode
    // -> v-for keeps reference to original_node.v-memo
    // -> children are visited (original_node is a child of ForNode)
    // -> transformMemo seen.has(original_node) === true, original_node is skipped
    // -> v-for post-processes together with remembered v-memo, using original_node VNodeCall
    // -> v-memo post-processes by wrapping original_node VNodeCall in withMemo, this wrapper is left
    // unused
    //
    // Fervid does flow differently, namely - its post hook skips work if original_node was
    // transformed:
    // - transformMemo(original_node) sets state.v_memo to `true` marking it for post-processing
    // -> v-for wraps original_node inside ForNode
    // -> children are visited (original_node is a child of ForNode)
    // -> transformMemo for a child (original_node) sets state.v_memo to `true` marking it for post-processing
    // -> transformMemo post-processing wraps original_node VNodeCall inside WithMemoCall
    // -> v-for post-processes the wrapped call by conditionally unwrapping it if it sees `v-for
    //    v-memo` usage
    // -> `memo` part of `WithMemoCall` gets remembered by ForNode codegen, VNodeCall gets assigned
    //    to original_node.
    // Fervid by design does not keep vuejs-core-style exit closures which reference original nodes
    // as that is a lot of work in Rust compared to JS. The necessary information is preserved
    // inside IR and transform state instead.
    //
    // Condition for v-memo promotion here is as follows:
    // either `state.v_memo` or `codegen_node.is_template` must be `true`, but not both.
    // This corresponds to 2 cases: `<div v-for v-memo>` and `<template v-for><div v-memo>...</div></template>`.
    // The nested v-memo is not supported by vuejs-core: https://github.com/vuejs/core/issues/4262
    if (state.v_memo ^ for_codegen_node.is_template)
        && let [Node::Element(element)] = for_node.children.as_mut_slice()
        && let Some(mut child_codegen) = element.codegen_node.take_if(|codegen_node| {
            matches!(codegen_node.value, ElementCodegenValue::WithMemoCall(_))
        })
    {
        let ElementCodegenValue::WithMemoCall(with_memo_call) = child_codegen.value else {
            // SAFETY: Checked above
            unreachable!();
        };

        // Do a sanity check - post_transform_for should be the only one assigning `memo`
        debug_assert!(
            for_codegen_node.memo.is_none(),
            "codegen_node memo was assigned already"
        );

        child_codegen.value = ElementCodegenValue::VNodeCall(Box::new(with_memo_call.vnode_call));
        element.codegen_node = Some(child_codegen);
        for_codegen_node.memo = Some(fervid_core::ForCodegenNodeMemo {
            expr: with_memo_call.expr,
            key: for_codegen_node.key.to_owned(),
        });
    }

    match for_node.children.as_mut_slice() {
        // <slot v-for="item in items" />
        // <template v-for="item in items"><slot /></template>
        // <div v-for="item in items">children...</div>
        [Node::Element(element_node)] => {
            let Some(element_codegen_node) = element_node.codegen_node.as_mut() else {
                return;
            };
            match &mut element_codegen_node.value {
                // Normal element v-for. Directly use the child's codegenNode
                // but mark it as a block.
                ElementCodegenValue::VNodeCall(v_node_call) => {
                    if for_codegen_node.is_template
                        && let Some(key) = for_codegen_node.key.take()
                    {
                        let span = key.span();
                        let prop = fervid_core::create_object_property(
                            fervid_core::create_simple_expression_propname(
                                fervid_core::fervid_atom!("key"),
                                true,
                                span,
                            )
                            .into(),
                            PropsExpression::from(key.as_ref()).into(),
                        );
                        inject_prop(&mut v_node_call.props, prop, &mut ctx.bindings_helper);
                    }

                    let is_stable = for_codegen_node
                        .patch_flags
                        .contains(PatchFlags::StableFragment);

                    let should_use_block = !is_stable || v_node_call.is_block_required;
                    v_node_call.is_block = should_use_block;

                    if !should_use_block && v_node_call.needs_patch {
                        v_node_call.patch_hints.flags |= PatchFlags::NeedPatch;
                    }
                }

                // <slot v-for="..."> or <template v-for="..."><slot/></template>
                ElementCodegenValue::SlotOutletCall(slot_outlet_call) => {
                    // If the slot already has a user-provided key, avoid injecting the branch
                    // key so the user key keeps its existing priority.
                    if for_codegen_node.is_template {
                        if !has_unnormalized_prop(slot_outlet_call.props.as_ref(), "key") {
                            slot_outlet_call.branch_key = for_codegen_node.key.take();
                        } else {
                            for_codegen_node.key = None;
                        }
                    }
                }

                // <template v-for="..." v-memo="..." :key="...">
                ElementCodegenValue::WithMemoCall(with_memo_call) => {
                    if let Some(key) = for_codegen_node.key.take() {
                        let span = key.span();
                        let prop = fervid_core::create_object_property(
                            fervid_core::create_simple_expression_propname(
                                fervid_core::fervid_atom!("key"),
                                true,
                                span,
                            )
                            .into(),
                            PropsExpression::from(key.as_ref()).into(),
                        );
                        inject_prop(
                            &mut with_memo_call.vnode_call.props,
                            prop,
                            &mut ctx.bindings_helper,
                        );
                    }

                    let is_stable = for_codegen_node
                        .patch_flags
                        .contains(PatchFlags::StableFragment);

                    let should_use_block =
                        !is_stable || with_memo_call.vnode_call.is_block_required;
                    with_memo_call.vnode_call.is_block = should_use_block;

                    if !should_use_block && with_memo_call.vnode_call.needs_patch {
                        with_memo_call.vnode_call.patch_hints.flags |= PatchFlags::NeedPatch;
                    }
                }
            }
        }

        // Need Fragment wrapper:
        // <template v-for="item in items">children...</template>
        _ => {
            // TODO This is currently done in `generate_for_node` in fervid_codegen.
            // Figure out if it is better moved here instead.
        }
    }
}

pub fn process_for(
    ctx: &mut TransformSfcContext,
    state: &mut TransformNodeState,
    node: &mut Node,
    mut v_for: VForDirective,
) {
    let Node::Element(_) = node else {
        return;
    };

    // `process_for` own body (pre)

    // The transform uses currently active scope as parent
    let parent_scope = state.current_scope;

    // Transform the for expression values
    finalize_for_parse_result(ctx, &mut v_for.parse_result, parent_scope);

    // Create new template scope
    let new_scope_to_use = ctx.bindings_helper.template_scopes.len() as u32;
    ctx.bindings_helper
        .template_scopes
        .push(crate::TemplateScope {
            variables: SmallVec::new(),
            parent: parent_scope,
        });

    // Replace original node with ForNode
    let original_node = std::mem::replace(
        node,
        Node::For(ForNode {
            parse_result: v_for.parse_result,
            children: vec![],
            template_scope: new_scope_to_use,
            codegen_node: None,
            span: v_for.span,
        }),
    );

    let Node::For(for_node) = node else {
        // SAFETY - We just replaced the node above with ForNode
        unreachable!()
    };

    // Assign ForNode children and return mutable reference to original ElementNode
    let mut template_carrier;
    let original_element_node = match original_node {
        Node::Element(mut original_element_node)
            if matches!(original_element_node.tag_type, ElementKind::Template) =>
        {
            let children = std::mem::take(&mut original_element_node.children);
            for_node.children = children;
            template_carrier = Some(original_element_node);
            if let Some(result) = template_carrier.as_mut() {
                result
            } else {
                // SAFETY - Value was assigned above
                unreachable!()
            }
        }

        _ => {
            let pushed = for_node.children.push_mut(original_node);
            let Node::Element(original_element_node) = pushed else {
                // SAFETY - This function only works on ElementNodes
                unreachable!()
            };
            original_element_node
        }
    };

    // Update the scopes
    ctx.directive_scopes.v_for += 1;
    state.current_scope = new_scope_to_use;
    state.for_scope = Some(parent_scope);

    // Get the iterator variables and collect their variables
    let scope = &mut ctx.bindings_helper.template_scopes[new_scope_to_use as usize];
    collect_variables(&for_node.parse_result.value, scope);
    if let Some(key) = &for_node.parse_result.key {
        collect_variables(key, scope);
    }
    if let Some(index) = &for_node.parse_result.index {
        collect_variables(index, scope);
    }

    // Codegen phase of process_for (pre)

    // `<template>` carriers get fully replaced by their children
    let is_template = matches!(original_element_node.tag_type, ElementKind::Template);

    // transformOnce already entered its scope before transformFor replaces this element
    let has_v_once = original_element_node
        .starting_tag
        .directives
        .as_mut()
        .and_then(|directives| directives.v_once.take())
        .is_some();

    // Find the `key` or `:key` on the original node
    let (has_key, mut key) = find_for_key(original_element_node, is_template);

    // :key and v-memo expressions need to be
    // processed on `<template v-for>`. In this case the node is discarded
    // and never traversed so its binding expressions won't be processed
    // by the normal transforms.
    // https://github.com/vuejs/core/issues/2085
    // https://github.com/vuejs/core/issues/5288

    // Transform the key in the following 2 cases:
    // 1. `<template v-for>` - because `<template>` is discarded.
    // 2. `<div v-for v-memo>` - because `find_for_key` cloned the raw value
    //    as `transform_expression` node transform has not run yet.
    if (is_template || state.v_memo)
        && let Some(key) = key.as_mut()
    {
        ctx.bindings_helper.transform_expr(key, new_scope_to_use);
    }

    let mut memo = None;
    if is_template
        && state.v_memo
        && let Some(mut memo_expr) = original_element_node
            .starting_tag
            .directives
            .as_mut()
            .and_then(|d| d.v_memo.take())
    {
        ctx.bindings_helper
            .transform_expr(&mut memo_expr, new_scope_to_use);

        memo = Some(fervid_core::ForCodegenNodeMemo {
            expr: memo_expr,
            key: key.to_owned(),
        })
    }

    // Compute the patch flags
    let is_stable =
        for_node.parse_result.source_const_type != fervid_core::ConstantTypes::NotConstant;
    let patch_flags = if is_stable {
        PatchFlags::StableFragment.into()
    } else if has_key {
        PatchFlags::KeyedFragment.into()
    } else {
        PatchFlags::UnkeyedFragment.into()
    };

    let codegen_node = Box::new(ForCodegenNode {
        patch_flags,
        disable_tracking: !is_stable,
        is_template,
        key,
        cache: if has_v_once {
            CacheMarker::InVOnce.into()
        } else {
            Default::default()
        },
        memo,
        branch_key: None,
    });

    for_node.codegen_node = Some(codegen_node);

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

fn find_for_key(node: &mut ElementNode, allow_taking: bool) -> (bool, Option<Box<Expr>>) {
    // Find match index
    let match_idx = node
        .starting_tag
        .attributes
        .iter()
        .position(|attribute| match attribute {
            AttributeOrBinding::RegularAttribute { name, .. } if name == "key" => true,
            AttributeOrBinding::VBind(v_bind) if matches!(v_bind.argument.as_ref(), Some(StrOrExpr::Str(name)) if name.value == "key") => true,
            AttributeOrBinding::VBind(v_bind) if matches!(v_bind.argument.as_ref(), Some(StrOrExpr::Expr(expr)) if matches!(expr.as_ref(), Expr::Lit(Lit::Str(name)) if name.value == "key")) => true,
            _ => false,
        });

    let Some(match_idx) = match_idx else {
        return (false, None);
    };

    // Since template carrier gets discarded, it is safe to take the values directly
    let matched_value = if allow_taking {
        Cow::Owned(node.starting_tag.attributes.swap_remove(match_idx))
    } else {
        let Some(attribute) = node.starting_tag.attributes.get(match_idx) else {
            return (false, None);
        };
        Cow::Borrowed(attribute)
    };

    match matched_value {
        Cow::Owned(AttributeOrBinding::VBind(v_bind)) => (true, Some(v_bind.value)),
        Cow::Borrowed(AttributeOrBinding::VBind(v_bind)) => (true, Some(v_bind.value.clone())),
        Cow::Owned(AttributeOrBinding::RegularAttribute { value, span, .. }) => (
            true,
            value.and_then(|value| {
                if value.is_empty() {
                    None
                } else {
                    Some(Box::new(Expr::Lit(Lit::Str(Str {
                        span,
                        value,
                        raw: None,
                    }))))
                }
            }),
        ),
        Cow::Borrowed(AttributeOrBinding::RegularAttribute { value, span, .. }) => (
            true,
            value.as_ref().and_then(|value| {
                if value.is_empty() {
                    None
                } else {
                    Some(Box::new(Expr::Lit(Lit::Str(Str {
                        span: *span,
                        value: value.to_owned(),
                        raw: None,
                    }))))
                }
            }),
        ),
        _ => (false, None),
    }
}

pub fn finalize_for_parse_result(
    ctx: &mut TransformSfcContext,
    result: &mut ForParseResult,
    scope_to_use: u32,
) {
    if result.finalized {
        return;
    }

    let metadata = ctx
        .bindings_helper
        .transform_expr(&mut result.source, scope_to_use);
    result.finalized_is_dynamic = metadata.has_js_bindings;
    result.source_const_type = metadata.const_type;

    result.finalized = true;
}

#[cfg(all(test, feature = "new-pipeline"))]
mod tests {
    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vFor.spec.ts#L921-L1030
    use fervid_core::{
        ElementCodegenNode, ElementNode, IntoIdent, PatchHints, PropsExpression, SlotOutletCall,
        SlotOutletFallback, StartingTag, VBindDirective, VNodeCall, VNodeCallTag, VueDirectives,
        WithMemoCall, fervid_atom,
    };
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{element_from_tag, js, to_str};

    use super::*;

    fn ident(name: &str) -> Box<Expr> {
        Box::new(Expr::Ident(
            fervid_core::FervidAtom::from(name).into_ident(),
        ))
    }

    fn loop_with_child(codegen: ElementCodegenValue) -> Node {
        let mut child = ElementNode::new_with_children_and_type(
            StartingTag {
                tag_name: "div".into(),
                attributes: vec![],
                directives: None,
            },
            vec![],
            ElementKind::Element,
        );
        child.codegen_node = Some(Box::new(ElementCodegenNode {
            value: codegen,
            cache: Default::default(),
        }));
        Node::For(ForNode {
            parse_result: Box::new(ForParseResult {
                source: ident("items"),
                value: ident("item"),
                key: None,
                index: None,
                finalized: true,
                finalized_is_dynamic: true,
                source_const_type: Default::default(),
            }),
            children: vec![Node::Element(child)],
            template_scope: 0,
            codegen_node: Some(Box::new(ForCodegenNode {
                patch_flags: PatchFlags::KeyedFragment.into(),
                disable_tracking: true,
                is_template: true,
                key: Some(ident("item")),
                cache: Default::default(),
                memo: None,
                branch_key: None,
            })),
            span: DUMMY_SP,
        })
    }

    fn post_transform(node: &mut Node) {
        post_transform_with_memo(node, false);
    }

    fn post_transform_with_memo(node: &mut Node, has_memo: bool) {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.directive_scopes.v_for = 1;
        let mut state = TransformNodeState {
            for_scope: Some(0),
            v_memo: has_memo,
            ..Default::default()
        };
        post_transform_for(&mut ctx, &mut state, node);
        assert_eq!(ctx.directive_scopes.v_for, 0);
    }

    fn memoized_child(memo: &str) -> ElementCodegenValue {
        ElementCodegenValue::WithMemoCall(Box::new(WithMemoCall {
            expr: js(memo),
            vnode_call: VNodeCall {
                tag: VNodeCallTag::Expr(js("'div'")),
                props: None,
                children: None,
                patch_hints: PatchHints::default(),
                directives: None,
                needs_patch: false,
                is_block_required: false,
                is_block: false,
                disable_tracking: false,
                is_component: false,
            },
        }))
    }

    #[test]
    fn find_for_key_moves_template_key_and_preserves_rendered_element_key() {
        // <template v-for="item in items" :key="item.id"><div /></template>
        // <div v-for="item in items" :key="item.id" />
        for (tag, take_key) in [("template", true), ("div", false)] {
            let mut element = element_from_tag(tag);
            element
                .starting_tag
                .attributes
                .push(AttributeOrBinding::VBind(VBindDirective {
                    argument: Some(fervid_atom!("key").into()),
                    value: js("item.id"),
                    is_camel: false,
                    is_prop: false,
                    is_attr: false,
                    span: DUMMY_SP,
                }));
            element
                .starting_tag
                .attributes
                .push(AttributeOrBinding::RegularAttribute {
                    name: fervid_atom!("title"),
                    value: Some(fervid_atom!("row")),
                    span: DUMMY_SP,
                });

            let (has_key, key) = find_for_key(&mut element, take_key);

            assert!(has_key);
            assert_eq!(
                to_str(key.as_deref().expect("key expression should exist")),
                "item.id"
            );
            assert_eq!(
                element.starting_tag.attributes.len(),
                if take_key { 1 } else { 2 }
            );
            assert!(element.starting_tag.attributes.iter().any(
                |attribute| matches!(attribute, AttributeOrBinding::RegularAttribute { name, .. } if name == "title")
            ));
        }
    }

    #[test]
    fn v_for_memo_moves_child_wrapper_into_loop_metadata() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L58-L66
        // <div v-for="item in items" :key="item" v-memo="[item.rev]" />
        let mut node = loop_with_child(memoized_child("[item.rev]"));
        let Node::For(for_node) = &mut node else {
            panic!("expected for node");
        };
        let codegen = for_node
            .codegen_node
            .as_mut()
            .expect("loop needs codegen metadata");
        codegen.is_template = false;
        let [Node::Element(child)] = for_node.children.as_mut_slice() else {
            panic!("normal v-for should retain its element child");
        };
        let Some(ElementCodegenNode {
            value: ElementCodegenValue::WithMemoCall(wrapper),
            ..
        }) = child.codegen_node.as_deref_mut()
        else {
            panic!("memo transform should wrap child vnode");
        };
        wrapper.vnode_call.props = Some(PropsExpression::ObjectExpression(Box::new(
            fervid_core::create_object_expression(
                vec![fervid_core::create_object_property(
                    fervid_core::create_simple_expression_propname(
                        fervid_atom!("key"),
                        true,
                        DUMMY_SP,
                    )
                    .into(),
                    PropsExpression::from(ident("item").as_ref()).into(),
                )],
                DUMMY_SP,
            ),
        )));

        post_transform_with_memo(&mut node, true);

        let Node::For(for_node) = node else {
            panic!("expected for node");
        };
        let codegen = for_node.codegen_node.expect("loop needs codegen metadata");
        let memo = codegen.memo.expect("loop should own memo expression");
        assert_eq!(to_str(memo.expr.as_ref()), "[item.rev]");
        let key = memo
            .key
            .expect("keyed loop memo should retain comparison key");
        assert_eq!(to_str(key.as_ref()), "item");
        let [Node::Element(child)] = for_node.children.as_slice() else {
            panic!("normal v-for should retain its element child");
        };
        let Some(ElementCodegenNode {
            value: ElementCodegenValue::VNodeCall(vnode),
            ..
        }) = child.codegen_node.as_deref()
        else {
            panic!("loop child should contain unwrapped VNodeCall");
        };
        assert!(vnode.is_block);
        assert!(has_unnormalized_prop(vnode.props.as_ref(), "key"));
    }

    #[test]
    fn template_v_for_promotes_sole_child_memo_and_injects_carrier_key() {
        // Fervid extension: Vue does not support child-owned v-memo inside v-for
        // https://github.com/vuejs/core/issues/4262
        // <template v-for="item in items" :key="item"><div v-memo="[item.rev]" /></template>
        let mut node = loop_with_child(memoized_child("[item.rev]"));

        post_transform(&mut node);

        let Node::For(for_node) = node else {
            panic!("expected for node");
        };
        let codegen = for_node.codegen_node.expect("loop needs codegen metadata");
        let memo = codegen
            .memo
            .expect("sole child memo should become loop memo");
        assert_eq!(to_str(memo.expr.as_ref()), "[item.rev]");
        let key = memo.key.expect("template memo should retain carrier key");
        assert_eq!(to_str(key.as_ref()), "item");
        assert!(
            codegen.key.is_none(),
            "carrier key should move into child vnode"
        );
        let [Node::Element(child)] = for_node.children.as_slice() else {
            panic!("expected sole element child");
        };
        let Some(ElementCodegenNode {
            value: ElementCodegenValue::VNodeCall(vnode),
            ..
        }) = child.codegen_node.as_deref()
        else {
            panic!("promoted child memo should leave a VNodeCall");
        };
        assert!(vnode.is_block);
        assert!(has_unnormalized_prop(vnode.props.as_ref(), "key"));
    }

    #[test]
    fn template_v_for_key_injection_with_single_child() {
        // <template v-for="item in items" :key="item"><div /></template>
        let mut node = loop_with_child(ElementCodegenValue::VNodeCall(Box::new(VNodeCall {
            tag: VNodeCallTag::Expr(Box::new(Expr::Lit("div".into()))),
            props: None,
            children: None,
            patch_hints: PatchHints::default(),
            directives: None,
            needs_patch: false,
            is_block_required: false,
            is_block: false,
            disable_tracking: false,
            is_component: false,
        })));
        post_transform(&mut node);
        let Node::For(for_node) = node else {
            panic!("expected for node")
        };
        assert!(
            for_node
                .codegen_node
                .as_ref()
                .is_some_and(|codegen| codegen.key.is_none())
        );
        let [Node::Element(element)] = for_node.children.as_slice() else {
            panic!("expected sole element")
        };
        let Some(ElementCodegenNode {
            value: ElementCodegenValue::VNodeCall(vnode),
            ..
        }) = element.codegen_node.as_deref()
        else {
            panic!("expected vnode")
        };
        assert!(vnode.is_block);
        assert!(has_unnormalized_prop(vnode.props.as_ref(), "key"));
    }

    #[test]
    fn normal_v_for_memo_processes_comparison_key_in_loop_scope() {
        // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vMemo.spec.ts#L68-L76
        // <div v-for="item in items" :key="get(item, suffix)" v-memo="[item.rev]" />
        let mut element = ElementNode::new(StartingTag {
            tag_name: "div".into(),
            attributes: vec![AttributeOrBinding::VBind(VBindDirective {
                argument: Some(fervid_atom!("key").into()),
                value: js("get(item, suffix)"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            })],
            directives: Some(Box::new(VueDirectives {
                v_memo: Some(js("[item.rev]")),
                v_for: Some(VForDirective {
                    parse_result: Box::new(ForParseResult {
                        source: js("items"),
                        value: ident("item"),
                        key: None,
                        index: None,
                        finalized: false,
                        finalized_is_dynamic: false,
                        source_const_type: Default::default(),
                    }),
                    patch_flags: PatchFlags::KeyedFragment.into(),
                    span: DUMMY_SP,
                }),
                ..Default::default()
            })),
        });
        element.tag_type = ElementKind::Element;
        let mut node = Node::Element(element);
        let mut ctx = TransformSfcContext::anonymous();
        let mut state = TransformNodeState {
            v_memo: true,
            ..Default::default()
        };

        pre_transform_for(&mut ctx, &mut state, &mut node);

        let Node::For(for_node) = node else {
            panic!("v-for should replace original element");
        };
        let key = for_node
            .codegen_node
            .as_ref()
            .and_then(|codegen| codegen.key.as_deref())
            .expect("loop should retain key for memo comparison");
        assert_eq!(to_str(key), "_ctx.get(item,_ctx.suffix)");
    }

    #[test]
    fn template_v_for_slot_key_respects_user_prop() {
        // <template v-for="item in items" :key="item"><slot :key="user" /></template>
        let props =
            PropsExpression::ObjectExpression(Box::new(fervid_core::create_object_expression(
                vec![fervid_core::create_object_property(
                    fervid_core::create_simple_expression_propname(
                        fervid_atom!("key"),
                        true,
                        DUMMY_SP,
                    )
                    .into(),
                    PropsExpression::from(ident("user").as_ref()).into(),
                )],
                DUMMY_SP,
            )));
        let mut node = loop_with_child(ElementCodegenValue::SlotOutletCall(Box::new(
            SlotOutletCall {
                name: "default".into(),
                props: Some(props),
                fallback: SlotOutletFallback::None,
                no_slotted: false,
                branch_key: None,
                span: DUMMY_SP,
            },
        )));
        post_transform(&mut node);
        let Node::For(for_node) = node else {
            panic!("expected for node")
        };
        assert!(
            for_node
                .codegen_node
                .as_ref()
                .is_some_and(|codegen| codegen.key.is_none())
        );
        let [Node::Element(element)] = for_node.children.as_slice() else {
            panic!("expected sole slot")
        };
        let Some(ElementCodegenNode {
            value: ElementCodegenValue::SlotOutletCall(slot),
            ..
        }) = element.codegen_node.as_deref()
        else {
            panic!("expected slot")
        };
        assert!(slot.branch_key.is_none());
        assert!(has_unnormalized_prop(slot.props.as_ref(), "key"));
    }

    #[test]
    fn template_v_for_slot_key_uses_branch_key() {
        // <template v-for="item in items" :key="item"><slot /></template>
        let mut node = loop_with_child(ElementCodegenValue::SlotOutletCall(Box::new(
            SlotOutletCall {
                name: "default".into(),
                props: None,
                fallback: SlotOutletFallback::None,
                no_slotted: false,
                branch_key: None,
                span: DUMMY_SP,
            },
        )));
        post_transform(&mut node);
        let Node::For(for_node) = node else {
            panic!("expected for node")
        };
        assert!(
            for_node
                .codegen_node
                .as_ref()
                .is_some_and(|codegen| codegen.key.is_none())
        );
        let [Node::Element(element)] = for_node.children.as_slice() else {
            panic!("expected sole slot")
        };
        let Some(ElementCodegenNode {
            value: ElementCodegenValue::SlotOutletCall(slot),
            ..
        }) = element.codegen_node.as_deref()
        else {
            panic!("expected slot")
        };
        assert!(matches!(slot.branch_key.as_deref(), Some(Expr::Ident(key)) if key.sym == "item"));
    }

    #[test]
    fn keyed_template_v_for_with_multiple_children_keeps_fragment_key() {
        // <template v-for="item in items" :key="item">text<div /></template>
        let mut node = loop_with_child(ElementCodegenValue::SlotOutletCall(Box::new(
            SlotOutletCall {
                name: "default".into(),
                props: None,
                fallback: SlotOutletFallback::None,
                no_slotted: false,
                branch_key: None,
                span: DUMMY_SP,
            },
        )));
        let Node::For(for_node) = &mut node else {
            panic!("expected for node")
        };
        for_node
            .children
            .insert(0, Node::Text("text".into(), DUMMY_SP));
        post_transform(&mut node);
        let Node::For(for_node) = node else {
            panic!("expected for node")
        };
        assert!(matches!(
            for_node.codegen_node.as_ref().and_then(|codegen| codegen.key.as_deref()),
            Some(Expr::Ident(key)) if key.sym == "item"
        ));
    }
}
