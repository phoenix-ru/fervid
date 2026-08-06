// Adapted from https://github.com/vuejs/core/blob/5a8aa0b2ba575e098cbb63b396e9bcb751eb3a0f/packages/compiler-core/src/ast.ts

use smallvec::SmallVec;
use swc_core::{
    common::{DUMMY_SP, Span},
    ecma::ast::{ArrayLit, Bool, CallExpr, Expr, IdentName, Lit, Pat, PropName, PropOrSpread, Str},
};

use crate::{BuiltinType, FervidAtom, PatchFlagsSet, PatchHints, StrOrExpr, VueImports};

#[derive(Debug, Clone, Default)]
pub enum ConstantTypes {
    #[default]
    NotConstant = 0,
    CanSkipPatch,
    CanCache,
    CanStringify,
}

#[derive(Debug, Clone)]
pub struct SimpleExpressionNode {
    pub ast: Box<Expr>,
    pub is_static: bool,
    pub const_type: ConstantTypes,
    pub is_handler_key: bool,
}

#[derive(Debug, Clone)]
pub struct CompoundExpressionNode {
    pub ast: Box<Expr>,
    pub is_handler_key: bool,
}

#[derive(Debug, Clone)]
pub enum ExpressionNode {
    SimpleExpression(SimpleExpressionNode),
    CompoundExpression(CompoundExpressionNode),
}

#[derive(Debug, Clone)]
pub struct SimpleExpressionPropNameNode {
    pub ast: IdentName,
    pub is_static: bool,
    pub const_type: ConstantTypes,
    pub is_handler_key: bool,
}

#[derive(Debug, Clone)]
pub struct CompoundExpressionPropNameNode {
    pub ast: PropName,
    pub is_handler_key: bool,
}

#[derive(Debug, Clone)]
pub enum ExpressionPropNameNode {
    SimpleExpression(SimpleExpressionPropNameNode),
    CompoundExpression(CompoundExpressionPropNameNode),
}

#[derive(Debug, Clone)]
pub enum VNodeCallTag {
    CallExpression(CallExpression),
    Builtin(BuiltinType),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum PropsExpression {
    ObjectExpression(Box<ObjectExpression>),
    CallExpression(Box<CallExpression>),
    ExpressionNode(Box<ExpressionNode>),
}

#[derive(Debug, Clone)]
pub enum VNodeChildren {
    /// Use the children from parent element
    UseElementChildren,
    /// Use the first and only child (which is a text node) from parent element
    UseFirstChildTextNode,
    /// Build component slots from the parent element's children.
    Slots(Slots),
}

#[derive(Debug, Clone)]
pub struct Slots {
    pub slots: Vec<SlotBuild>,
    pub dynamic_slots: Vec<DynamicSlot>,
    pub has_dynamic_slots: bool,
    pub slot_flag: SlotFlag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotFlag {
    Stable = 1,
    Dynamic = 2,
    Forwarded = 3,
}

#[derive(Debug, Clone)]
pub struct SlotBuild {
    pub name: StrOrExpr,
    pub props: Option<Box<Pat>>,
    pub source: SlotSource,
}

#[derive(Debug, Clone)]
pub enum DynamicSlot {
    Conditional(DynamicSlotConditional),
    RenderList(DynamicSlotRenderList),
}

#[derive(Debug, Clone)]
pub struct DynamicSlotConditional {
    pub if_slot: ConditionalDynamicSlot,
    pub else_if_slots: Vec<ConditionalDynamicSlot>,
    pub else_slot: Option<DynamicSlotBuild>,
}

#[derive(Debug, Clone)]
pub struct ConditionalDynamicSlot {
    pub condition: Box<Expr>,
    pub slot: DynamicSlotBuild,
}

#[derive(Debug, Clone)]
pub struct DynamicSlotRenderList {
    /// Index points to a `<template v-for v-slot>` carrier node in parent component children.
    /// Codegen can read the `VForDirective` from that node and render `slot` for each item.
    pub slot_template_index: usize,
    pub slot: DynamicSlotBuild,
}

#[derive(Debug, Clone)]
pub struct DynamicSlotBuild {
    pub name: StrOrExpr,
    pub props: Option<Box<Pat>>,
    pub source: SlotSource,
    pub key: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum SlotSource {
    /// Indices point to parent component children that render directly as the default slot.
    /// They are captured after child-shaping post transforms have run.
    ImplicitDefaultSlot(SmallVec<[usize; 1]>),
    /// Index points to a `<template v-slot>` carrier node in the parent component children.
    /// Codegen must render that template node's children, not the template node itself.
    TemplateSlotChildren(usize),
}

#[derive(Debug, Clone)]
pub struct VNodeCall {
    pub tag: VNodeCallTag,
    pub props: Option<PropsExpression>,
    pub children: Option<VNodeChildren>,
    pub patch_hints: PatchHints,
    pub directives: Option<ArrayLit>,
    /// Whether this vnode must be patched if a later transform makes it non-block
    pub needs_patch: bool,
    /// Whether a later transform must preserve this vnode as a block
    pub is_block_required: bool,
    pub is_block: bool,
    pub disable_tracking: bool,
    pub is_component: bool,
}

/// Outer Fragment VNodeCall generated for v-for
#[derive(Debug, Clone)]
pub struct ForCodegenNode {
    pub patch_flags: PatchFlagsSet,
    pub disable_tracking: bool,
    pub is_template: bool,
    pub key: Option<Box<Expr>>,
}

// JS Node Types

#[derive(Debug, Clone)]
pub enum JsChildNode {
    CallExpression(Box<CallExpression>),
    ObjectExpression(Box<ObjectExpression>),
    ExpressionNode(Box<ExpressionNode>),
    ArrayExpression(Box<ArrayExpression>),
    // Unused?
    // OriginalValueMarker,
}

#[derive(Debug, Clone)]
pub struct CallExpression {
    pub callee: VueImports,
    pub span: Span,
    pub arguments: Vec<JsChildNode>,
}

#[derive(Debug, Clone)]
pub struct ObjectExpression {
    pub properties: Vec<Property>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub key: ExpressionPropNameNode,
    pub value: JsChildNode,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ArrayExpression {
    pub elements: Vec<JsChildNode>,
    pub span: Span,
}

pub fn create_call_expression(
    callee: VueImports,
    arguments: Vec<JsChildNode>,
    span: Span,
) -> CallExpression {
    CallExpression {
        callee,
        span,
        arguments,
    }
}

pub fn create_object_expression(properties: Vec<Property>, span: Span) -> ObjectExpression {
    ObjectExpression { properties, span }
}

pub fn create_object_property(key: ExpressionPropNameNode, value: JsChildNode) -> Property {
    Property {
        key,
        value,
        span: DUMMY_SP,
    }
}

pub fn create_array_expression(elements: Vec<JsChildNode>, span: Span) -> ArrayExpression {
    ArrayExpression { elements, span }
}

pub fn create_simple_expression_propname(
    content: FervidAtom,
    is_static: bool,
    span: Span,
) -> SimpleExpressionPropNameNode {
    SimpleExpressionPropNameNode {
        ast: IdentName { sym: content, span },
        is_static,
        const_type: if is_static {
            ConstantTypes::CanStringify
        } else {
            ConstantTypes::NotConstant
        },
        is_handler_key: false,
    }
}

pub fn create_simple_expression_bool(content: bool) -> SimpleExpressionNode {
    SimpleExpressionNode {
        ast: Box::new(Expr::Lit(Lit::Bool(Bool {
            value: content,
            span: DUMMY_SP,
        }))),
        is_static: true,
        const_type: ConstantTypes::CanStringify,
        is_handler_key: false,
    }
}

pub fn create_simple_expression_str(
    content: FervidAtom,
    is_static: bool,
    span: Span,
) -> SimpleExpressionNode {
    SimpleExpressionNode {
        ast: Box::new(Expr::Lit(Lit::Str(Str {
            span,
            value: content,
            raw: None,
        }))),
        is_static,
        const_type: if is_static {
            ConstantTypes::CanStringify
        } else {
            ConstantTypes::NotConstant
        },
        is_handler_key: false,
    }
}

// Property

// impl Into<PropOrSpread> for Property {
//     fn into(self) -> PropOrSpread {
//         PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
//             key: self.key.into(),
//             value: self.value.ast,
//         })))
//     }
// }

impl From<PropOrSpread> for Property {
    fn from(_value: PropOrSpread) -> Self {
        todo!()
        // Property {
        //     key: todo!(),
        //     value: todo!(),
        // }
    }
}

// CallExpression

impl From<CallExpr> for CallExpression {
    fn from(_value: CallExpr) -> Self {
        todo!()
        // Self {
        //     callee: (),
        //     span: (),
        //     arguments: (),
        // }
    }
}

// ExpressionNode

impl From<Expr> for ExpressionNode {
    fn from(value: Expr) -> Self {
        Self::CompoundExpression(CompoundExpressionNode {
            ast: Box::new(value),
            is_handler_key: false,
        })
    }
}

// ExpressionPropNameNode

impl ExpressionPropNameNode {
    pub fn is_handler_key(&self) -> bool {
        match self {
            ExpressionPropNameNode::SimpleExpression(s) => s.is_handler_key,
            ExpressionPropNameNode::CompoundExpression(c) => c.is_handler_key,
        }
    }
}

impl From<ExpressionPropNameNode> for PropName {
    fn from(val: ExpressionPropNameNode) -> Self {
        match val {
            ExpressionPropNameNode::SimpleExpression(s) => PropName::Ident(s.ast),
            ExpressionPropNameNode::CompoundExpression(c) => c.ast,
        }
    }
}

impl From<SimpleExpressionPropNameNode> for ExpressionPropNameNode {
    fn from(value: SimpleExpressionPropNameNode) -> Self {
        Self::SimpleExpression(value)
    }
}

// PropsExpression

impl From<&Expr> for PropsExpression {
    fn from(value: &Expr) -> Self {
        match value {
            Expr::Call(call_expr) => {
                PropsExpression::CallExpression(Box::new(call_expr.to_owned().into()))
            }
            Expr::Object(obj_expr) => {
                PropsExpression::ObjectExpression(Box::new(ObjectExpression {
                    properties: obj_expr.props.iter().cloned().map(Into::into).collect(),
                    span: obj_expr.span,
                }))
            }
            _ => PropsExpression::ExpressionNode(Box::new(value.to_owned().into())),
        }
    }
}

// JsChildNode

impl From<SimpleExpressionNode> for JsChildNode {
    fn from(value: SimpleExpressionNode) -> Self {
        Self::ExpressionNode(Box::new(ExpressionNode::SimpleExpression(value)))
    }
}

impl From<CallExpression> for JsChildNode {
    fn from(value: CallExpression) -> Self {
        Self::CallExpression(Box::new(value))
    }
}

impl From<PropsExpression> for JsChildNode {
    fn from(val: PropsExpression) -> Self {
        match val {
            PropsExpression::ObjectExpression(o) => JsChildNode::ObjectExpression(o),
            PropsExpression::CallExpression(c) => JsChildNode::CallExpression(c),
            PropsExpression::ExpressionNode(e) => JsChildNode::ExpressionNode(e),
        }
    }
}
