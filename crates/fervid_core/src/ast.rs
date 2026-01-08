// Adapted from https://github.com/vuejs/core/blob/5a8aa0b2ba575e098cbb63b396e9bcb751eb3a0f/packages/compiler-core/src/ast.ts

use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::ast::{Bool, CallExpr, Expr, IdentName, Lit, PropName, PropOrSpread, Str},
};

use crate::{FervidAtom, VueImports};

#[derive(Default)]
pub enum ConstantTypes {
    #[default]
    NotConstant = 0,
    CanSkipPatch,
    CanCache,
    CanStringify,
}

pub struct SimpleExpressionNode {
    pub ast: Box<Expr>,
    pub is_static: bool,
    pub const_type: ConstantTypes,
    pub is_handler_key: bool,
}

pub struct CompoundExpressionNode {
    pub ast: Box<Expr>,
    pub is_handler_key: bool,
}

pub enum ExpressionNode {
    SimpleExpression(SimpleExpressionNode),
    CompoundExpression(CompoundExpressionNode),
}

pub struct SimpleExpressionPropNameNode {
    pub ast: IdentName,
    pub is_static: bool,
    pub const_type: ConstantTypes,
    pub is_handler_key: bool,
}

pub struct CompoundExpressionPropNameNode {
    pub ast: PropName,
    pub is_handler_key: bool,
}

pub enum ExpressionPropNameNode {
    SimpleExpression(SimpleExpressionPropNameNode),
    CompoundExpression(CompoundExpressionPropNameNode),
}

// JS Node Types

pub enum JsChildNode {
    CallExpression(Box<CallExpression>),
    ObjectExpression(Box<ObjectExpression>),
    ExpressionNode(Box<ExpressionNode>),
    // Unused?
    OriginalValueMarker,
}

pub struct CallExpression {
    pub callee: VueImports,
    pub span: Span,
    pub arguments: Vec<JsChildNode>,
}

pub struct ObjectExpression {
    pub properties: Vec<Property>,
    pub span: Span,
}

pub struct Property {
    pub key: ExpressionPropNameNode,
    pub value: JsChildNode,
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
    Property { key, value }
}

pub fn create_simple_expression_propname(
    content: FervidAtom,
    is_static: bool,
    span: Span,
) -> SimpleExpressionPropNameNode {
    SimpleExpressionPropNameNode {
        ast: IdentName { sym: content, span },
        is_static,
        const_type: Default::default(),
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
