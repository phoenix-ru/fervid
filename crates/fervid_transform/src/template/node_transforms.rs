use std::fmt::Debug;

use enum_dispatch::enum_dispatch;
use fervid_core::{ElementKind, Node};

use crate::{
    TransformSfcContext,
    template::{
        core::{
            transform_expression::pre_transform_expression,
            transform_if::transform_if,
            transform_whitespace::transform_whitespace,
            v_for::{post_transform_for, pre_transform_for},
            v_once::{post_transform_once, pre_transform_once},
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
        scope_to_use: u32,
    );
    fn pre_transform_node(
        &self,
        ctx: &mut TransformSfcContext,
        node: &mut Node,
        parent_scope: u32,
    ) -> TransformNodeState;
    fn post_transform_node(
        &self,
        ctx: &mut TransformSfcContext,
        node: &mut Node,
        state: &mut TransformNodeState,
    );
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

#[derive(Debug, Default)]
pub struct TransformNodeState {
    // Scope management for template identifiers
    pub current_scope: u32,
    pub parent_scope: u32,

    // Node transform states

    // v-slot
    /// Some(value) marks the previous current scope
    pub slot_scopes: Option<u32>,
    /// Some(value) marks the previous current scope
    pub v_for_slot_scopes: Option<u32>,

    // v-for
    /// Some(value) marks the previous current scope
    pub for_scope: Option<u32>,
}

impl TransformNodeState {
    pub fn new(parent_scope: u32) -> Self {
        TransformNodeState {
            current_scope: parent_scope,
            parent_scope,
            ..Default::default()
        }
    }
}

// Base transforms are provided as default trait implementations
impl NodeTransforms for BaseNodeTransform {
    fn pre_transform_children(
        &self,
        ctx: &mut TransformSfcContext,
        children: &mut Vec<Node>,
        element_kind: ElementKind,
        scope_to_use: u32,
    ) {
        // Whitespace handling happens during parser/transform normalization in Vue.
        transform_whitespace(children, element_kind);
        // transformIf,
        transform_if(ctx, children, scope_to_use);
    }

    fn pre_transform_node(
        &self,
        ctx: &mut TransformSfcContext,
        node: &mut Node,
        parent_scope: u32,
    ) -> TransformNodeState {
        let mut state = TransformNodeState::new(parent_scope);

        // transformOnce,
        pre_transform_once(ctx, node);
        // transformIf,
        pre_transform_if(ctx, node);
        // transformMemo,
        pre_transform_memo(ctx, node);
        // transformFor,
        pre_transform_for(ctx, &mut state, node);
        // prefixIdentifiers ? trackVForSlotScopes,
        super::core::v_slot::pre_track_v_for_slot_scopes(ctx, &mut state, node);
        // prefixIdentifiers ? transformExpression,
        pre_transform_expression(ctx, &state, node);
        // transformSlotOutlet,
        pre_transform_slot_outlet(ctx, node);

        // transformElement - no pre hook

        // trackSlotScopes,
        super::core::v_slot::pre_track_slot_scopes(ctx, &mut state, node);
        // transformText,
        pre_transform_text(ctx, node);
        // TODO - User transforms in the separate implementation?

        state
    }

    fn post_transform_node(
        &self,
        ctx: &mut TransformSfcContext,
        node: &mut Node,
        state: &mut TransformNodeState,
    ) {
        // Post transforms run in reverse order.
        // transformText,
        post_transform_text(ctx, node);
        // trackSlotScopes,
        super::core::v_slot::post_track_slot_scopes(ctx, state, node);

        // transformElement
        post_transform_element_node(node, ctx, state);

        // transformSlotOutlet,
        post_transform_slot_outlet(ctx, node);
        // prefixIdentifiers ? transformExpression,
        post_transform_expression(ctx, node);
        // prefixIdentifiers ? trackVForSlotScopes,
        super::core::v_slot::post_track_v_for_slot_scopes(ctx, state, node);
        // transformFor,
        post_transform_for(ctx, state, node);
        // transformMemo,
        post_transform_memo(ctx, node);
        // transformIf,
        post_transform_if(ctx, node);
        // transformOnce,
        post_transform_once(ctx, node);
        // TODO - User transforms in the separate implementation?
    }
}

fn pre_transform_if(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_if(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_memo(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_memo(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn post_transform_expression(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_slot_outlet(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_slot_outlet(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

fn pre_transform_text(_ctx: &mut TransformSfcContext, _node: &mut Node) {}
fn post_transform_text(_ctx: &mut TransformSfcContext, _node: &mut Node) {}

impl NodeTransforms for DomNodeTransform {
    fn pre_transform_children(
        &self,
        ctx: &mut TransformSfcContext,
        children: &mut Vec<Node>,
        element_kind: ElementKind,
        scope_to_use: u32,
    ) {
        BaseNodeTransform.pre_transform_children(ctx, children, element_kind, scope_to_use);
    }

    fn pre_transform_node(
        &self,
        ctx: &mut TransformSfcContext,
        node: &mut Node,
        parent_scope: u32,
    ) -> TransformNodeState {
        BaseNodeTransform.pre_transform_node(ctx, node, parent_scope)
        // Core node transforms;
        // ignoreSideEffectTags;
        // transformStyle;
        // DEV ? transformTransition;
        // DEV ? validateHtmlNesting;
    }

    fn post_transform_node(
        &self,
        ctx: &mut TransformSfcContext,
        node: &mut Node,
        state: &mut TransformNodeState,
    ) {
        // TODO: Post transforms run in reverse order
        BaseNodeTransform.post_transform_node(ctx, node, state);
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
        scope_to_use: u32,
    ) {
        BaseNodeTransform.pre_transform_children(ctx, children, element_kind, scope_to_use);
    }

    fn pre_transform_node(
        &self,
        _ctx: &mut TransformSfcContext,
        _node: &mut Node,
        parent_scope: u32,
    ) -> TransformNodeState {
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
        TransformNodeState::new(parent_scope)
    }

    fn post_transform_node(
        &self,
        _ctx: &mut TransformSfcContext,
        _node: &mut Node,
        _state: &mut TransformNodeState,
    ) {
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
