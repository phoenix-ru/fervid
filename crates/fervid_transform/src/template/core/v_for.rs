use fervid_core::{
    AttributeOrBinding, CacheMarker, ElementCodegenValue, ElementKind, ForCodegenNode, ForNode,
    ForParseResult, Node, PatchFlags, PropsExpression, StrOrExpr, VForDirective,
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

    let Some(codegen_node) = for_node.codegen_node.as_mut() else {
        return;
    };

    // Check <template v-for> key placement
    if !ctx.bindings_helper.is_prod && codegen_node.is_template {
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
                    if codegen_node.is_template
                        && let Some(key) = codegen_node.key.take()
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

                    let is_stable = codegen_node
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
                    if codegen_node.is_template {
                        if !has_unnormalized_prop(slot_outlet_call.props.as_ref(), "key") {
                            slot_outlet_call.branch_key = codegen_node.key.take();
                        } else {
                            codegen_node.key = None;
                        }
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

    // The transform uses currently active scope as parent
    let parent_scope = state.current_scope;

    // TODO: Move to codegen
    let (has_key, mut key) = find_for_key(element_node);

    // exclude moving
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
    let new_scope_to_use = ctx.bindings_helper.template_scopes.len() as u32;
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
            template_scope: new_scope_to_use,
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

    // TODO: Move to codegen phase
    if is_template && let Some(key) = key.as_mut() {
        ctx.bindings_helper.transform_expr(key, new_scope_to_use);
    }
    if let Some(codegen_node) = for_node.codegen_node.as_mut() {
        codegen_node.key = key;
    }

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
                let key = value.as_ref().and_then(|value| {
                    if value.is_empty() {
                        None
                    } else {
                        Some(Box::new(Expr::Lit(Lit::Str(Str {
                            span: *span,
                            value: value.clone(),
                            raw: None,
                        }))))
                    }
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

#[cfg(all(test, feature = "new-pipeline"))]
mod tests {
    // Vue source: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vFor.spec.ts#L921-L1030
    use fervid_core::{
        ElementCodegenNode, ElementNode, IntoIdent, PatchHints, PropsExpression, SlotOutletCall,
        SlotOutletFallback, StartingTag, VNodeCall, VNodeCallTag, fervid_atom,
    };
    use swc_core::common::DUMMY_SP;

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
            }),
            children: vec![Node::Element(child)],
            template_scope: 0,
            codegen_node: Some(Box::new(ForCodegenNode {
                patch_flags: PatchFlags::KeyedFragment.into(),
                disable_tracking: true,
                is_template: true,
                key: Some(ident("item")),
                cache: Default::default(),
            })),
            span: DUMMY_SP,
        })
    }

    fn post_transform(node: &mut Node) {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.directive_scopes.v_for = 1;
        let mut state = TransformNodeState {
            for_scope: Some(0),
            ..Default::default()
        };
        post_transform_for(&mut ctx, &mut state, node);
        assert_eq!(ctx.directive_scopes.v_for, 0);
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
