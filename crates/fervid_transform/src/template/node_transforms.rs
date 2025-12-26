use std::fmt::Debug;

use enum_dispatch::enum_dispatch;
use fervid_core::ElementNode;

use crate::{template::transform_element::post_transform_element_node, TransformSfcContext};

#[enum_dispatch]
pub trait NodeTransforms: Debug {
    fn pre_transform_element_node(&self, ctx: &mut TransformSfcContext, node: &mut ElementNode);
    fn post_transform_element_node(&self, ctx: &mut TransformSfcContext, node: &mut ElementNode);
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
        Self::Base(BaseNodeTransform::default())
    }
}

// Base transforms are provided as default trait implementations
impl NodeTransforms for BaseNodeTransform {
    fn pre_transform_element_node(&self, _ctx: &mut TransformSfcContext, _node: &mut ElementNode) {
        // transformOnce,
        // transformIf,
        // transformMemo,
        // transformFor,
        // prefixIdentifiers ? trackVForSlotScopes,
        // prefixIdentifiers ? transformExpression,
        // transformSlotOutlet,

        // transformElement - no pre hook

        // trackSlotScopes,
        // transformText,
        // TODO - User transforms in the separate implementation?
    }

    fn post_transform_element_node(&self, ctx: &mut TransformSfcContext, node: &mut ElementNode) {
        // transformOnce,
        // transformIf,
        // transformMemo,
        // transformFor,
        // prefixIdentifiers ? trackVForSlotScopes,
        // prefixIdentifiers ? transformExpression,
        // transformSlotOutlet,

        // transformElement
        post_transform_element_node(node, ctx);

        // trackSlotScopes,
        // transformText,
        // TODO - User transforms in the separate implementation?
    }
}

impl NodeTransforms for DomNodeTransform {
    fn pre_transform_element_node(&self, ctx: &mut TransformSfcContext, node: &mut ElementNode) {
        BaseNodeTransform.pre_transform_element_node(ctx, node);
        // Core node transforms;
        // ignoreSideEffectTags;
        // transformStyle;
        // DEV ? transformTransition;
        // DEV ? validateHtmlNesting;
    }

    fn post_transform_element_node(&self, ctx: &mut TransformSfcContext, node: &mut ElementNode) {
        BaseNodeTransform.post_transform_element_node(ctx, node);
        // Core node transforms;
        // ignoreSideEffectTags;
        // transformStyle;
        // DEV ? transformTransition;
        // DEV ? validateHtmlNesting;
    }
}

impl NodeTransforms for SsrNodeTransform {
    fn pre_transform_element_node(&self, _ctx: &mut TransformSfcContext, _node: &mut ElementNode) {
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

    fn post_transform_element_node(&self, _ctx: &mut TransformSfcContext, _node: &mut ElementNode) {
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
