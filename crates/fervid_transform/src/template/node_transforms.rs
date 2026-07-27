use std::fmt::Debug;

use enum_dispatch::enum_dispatch;
use fervid_core::{ElementKind, Node};

use crate::{
    TransformSfcContext,
    template::{
        core::{
            transform_if::transform_if,
            transform_whitespace::transform_whitespace,
            v_for::{post_transform_for, pre_transform_for},
        },
        transform_element::post_transform_element_node,
    },
};

#[enum_dispatch]
pub trait NodeTransforms: Debug {
    fn pre_transform_children(
        &self,
        _ctx: &mut TransformSfcContext,
        children: &mut Vec<Node>,
        element_kind: ElementKind,
    );
    fn pre_transform_node(&self, ctx: &mut TransformSfcContext, node: &mut Node);
    fn post_transform_node(&self, ctx: &mut TransformSfcContext, node: &mut Node);
}

// Transforms should not hold any data because they need to be copied,
// any state should be stored on the `&mut ctx` provided to the functions.

#[derive(Debug, Clone, Default)]
pub struct BaseNodeTransform;

#[derive(Debug, Clone, Default)]
pub struct DomNodeTransform;

#[derive(Debug, Clone, Default)]
pub struct SsrNodeTransform;

#[derive(Debug, Clone)]
#[enum_dispatch(NodeTransforms)]
pub enum NodeTransformsProvider {
    Base(BaseNodeTransform),
    Dom(DomNodeTransform),
    Ssr(SsrNodeTransform),
    // Note: Supporting custom transforms bloats the size of `NodeTransformsProvider` substantially
    // due to struct aligning.
    // Custom(Rc<dyn NodeTransforms>),
}

impl Default for NodeTransformsProvider {
    fn default() -> Self {
        Self::Base(BaseNodeTransform)
    }
}

// Base transforms are provided as default trait implementations
impl NodeTransforms for BaseNodeTransform {
    fn pre_transform_children(
        &self,
        _ctx: &mut TransformSfcContext,
        children: &mut Vec<Node>,
        element_kind: ElementKind,
    ) {
        // Whitespace handling happens during parser/transform normalization in Vue.
        transform_whitespace(children, element_kind);
        // transformIf,
        transform_if(children);
    }

    fn pre_transform_node(&self, ctx: &mut TransformSfcContext, node: &mut Node) {
        // transformOnce,
        pre_transform_once(ctx, node);
        // transformIf,
        pre_transform_if(ctx, node);
        // transformMemo,
        pre_transform_memo(ctx, node);
        // transformFor,
        pre_transform_for(ctx, node);
        // prefixIdentifiers ? trackVForSlotScopes,
        pre_track_v_for_slot_scopes(ctx, node);
        // prefixIdentifiers ? transformExpression,
        pre_transform_expression(ctx, node);
        // transformSlotOutlet,
        pre_transform_slot_outlet(ctx, node);

        // transformElement - no pre hook

        // trackSlotScopes,
        pre_track_slot_scopes(ctx, node);
        // transformText,
        pre_transform_text(ctx, node);
        // TODO - User transforms in the separate implementation?
    }

    fn post_transform_node(&self, ctx: &mut TransformSfcContext, node: &mut Node) {
        // Post transforms run in reverse order.
        // transformText,
        post_transform_text(ctx, node);
        // trackSlotScopes,
        post_track_slot_scopes(ctx, node);

        // transformElement
        post_transform_element_node(node, ctx);

        // transformSlotOutlet,
        post_transform_slot_outlet(ctx, node);
        // prefixIdentifiers ? transformExpression,
        post_transform_expression(ctx, node);
        // prefixIdentifiers ? trackVForSlotScopes,
        post_track_v_for_slot_scopes(ctx, node);
        // transformFor,
        post_transform_for(ctx, node);
        // transformMemo,
        post_transform_memo(ctx, node);
        // transformIf,
        post_transform_if(ctx, node);
        // transformOnce,
        post_transform_once(ctx, node);
        // TODO - User transforms in the separate implementation?
    }
}

fn pre_transform_once(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_once(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_if(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_if(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_memo(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_memo(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_track_v_for_slot_scopes(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_track_v_for_slot_scopes(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_expression(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_expression(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_slot_outlet(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_slot_outlet(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_track_slot_scopes(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_track_slot_scopes(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_text(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_text(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

impl NodeTransforms for DomNodeTransform {
    fn pre_transform_children(
        &self,
        ctx: &mut TransformSfcContext,
        children: &mut Vec<Node>,
        element_kind: ElementKind,
    ) {
        BaseNodeTransform.pre_transform_children(ctx, children, element_kind);
    }

    fn pre_transform_node(&self, ctx: &mut TransformSfcContext, node: &mut Node) {
        BaseNodeTransform.pre_transform_node(ctx, node);
        // Core node transforms;
        // ignoreSideEffectTags;
        // transformStyle;
        // DEV ? transformTransition;
        // DEV ? validateHtmlNesting;
    }

    fn post_transform_node(&self, ctx: &mut TransformSfcContext, node: &mut Node) {
        // TODO: Post transforms run in reverse order
        BaseNodeTransform.post_transform_node(ctx, node);
        // Core node transforms;
        // ignoreSideEffectTags;
        // transformStyle;
        // DEV ? transformTransition;
        // DEV ? validateHtmlNesting;
    }
}

impl NodeTransforms for SsrNodeTransform {
    fn pre_transform_children(
        &self,
        ctx: &mut TransformSfcContext,
        children: &mut Vec<Node>,
        element_kind: ElementKind,
    ) {
        BaseNodeTransform.pre_transform_children(ctx, children, element_kind);
    }

    fn pre_transform_node(&self, _ctx: &mut TransformSfcContext, _node: &mut Node) {
        // ssrTransformIf
        // ssrTransformFor
        // trackVForSlotScopes
        // transformExpression
        // ssrTransformSlotOutlet
        // ssrInjectFallthroughAttrs
        // ssrInjectCssVars
        // ssrTransformElement
        // ssrTransformComponent
        // trackSlotScopes
        // transformStyle
    }

    fn post_transform_node(&self, _ctx: &mut TransformSfcContext, _node: &mut Node) {
        // TODO: Post transforms run in reverse order
        // ssrTransformIf
        // ssrTransformFor
        // trackVForSlotScopes
        // transformExpression
        // ssrTransformSlotOutlet
        // ssrInjectFallthroughAttrs
        // ssrInjectCssVars
        // ssrTransformElement
        // ssrTransformComponent
        // trackSlotScopes
        // transformStyle
    }
}
