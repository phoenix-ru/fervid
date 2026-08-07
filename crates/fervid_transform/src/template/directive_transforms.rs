use std::fmt::Debug;

use enum_dispatch::enum_dispatch;
use fervid_core::{
    ElementNode, FervidAtom, Property, StrOrExpr, VBindDirective, VModelDirective, VOnDirective,
    VueImports,
};
use swc_core::ecma::ast::Expr;

use crate::{TransformSfcContext, template::core::v_bind::transform_v_bind};

pub struct DirectiveTransformResult {
    pub runtime_directive: Option<BuiltinRuntimeDirective>,
    pub props: Vec<Property>,
    pub remove_children: bool,
}

pub struct BuiltinRuntimeDirective {
    pub import: VueImports,
    pub value: Option<Box<Expr>>,
    pub arg: Option<StrOrExpr>,
    pub modifiers: Vec<FervidAtom>,
}

#[enum_dispatch]
pub trait DirectiveTransforms: Debug {
    fn transform_v_bind(
        &self,
        ctx: &mut TransformSfcContext,
        v_bind: &VBindDirective,
        node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        // TODO: Use ctx.in_ssr inside the transform when implemented
        transform_v_bind(ctx, v_bind, node, false)
    }

    fn transform_v_on(
        &self,
        _ctx: &mut TransformSfcContext,
        _v_on: &VOnDirective,
        _node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        None
    }

    fn transform_v_model(
        &self,
        _ctx: &mut TransformSfcContext,
        _v_model: &VModelDirective,
        _node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        None
    }

    fn transform_v_html(
        &self,
        _ctx: &mut TransformSfcContext,
        _v_html: &Expr,
        _node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        None
    }

    fn transform_v_text(
        &self,
        _ctx: &mut TransformSfcContext,
        _v_text: &Expr,
        _node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        None
    }

    fn transform_v_show(
        &self,
        _ctx: &mut TransformSfcContext,
        _v_show: &Expr,
        _node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        None
    }
}

// Transforms should not hold any data because they need to be copied,
// any state should be stored on the `&mut ctx` provided to the functions.

#[derive(Debug, Clone, Default)]
pub struct BaseDirectiveTransform;

#[derive(Debug, Clone, Default)]
pub struct DomDirectiveTransform;

#[derive(Debug, Clone)]
#[enum_dispatch(DirectiveTransforms)]
pub enum DirectiveTransformsProvider {
    Base(BaseDirectiveTransform),
    Dom(DomDirectiveTransform),
    // Ssr,
    // Note: Supporting custom transforms bloats the size of `DirectiveTransformsProvider` substantially
    // due to struct aligning.
    // Custom(Rc<dyn DirectiveTransforms>),
}

impl Default for DirectiveTransformsProvider {
    fn default() -> Self {
        Self::Dom(DomDirectiveTransform)
    }
}

// Base transforms are provided as default trait implementations
impl DirectiveTransforms for BaseDirectiveTransform {}

impl DirectiveTransforms for DomDirectiveTransform {
    fn transform_v_html(
        &self,
        ctx: &mut TransformSfcContext,
        v_html: &Expr,
        node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        super::dom::v_html::transform_v_html(ctx, v_html, node)
    }

    fn transform_v_text(
        &self,
        ctx: &mut TransformSfcContext,
        v_text: &Expr,
        node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        super::dom::v_text::transform_v_text(ctx, v_text, node)
    }

    fn transform_v_show(
        &self,
        ctx: &mut TransformSfcContext,
        v_show: &Expr,
        _node: &ElementNode,
    ) -> Option<DirectiveTransformResult> {
        super::dom::v_show::transform_v_show(ctx, v_show)
    }
}
