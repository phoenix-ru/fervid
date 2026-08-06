use fervid_core::{
    CompoundExpressionPropNameNode, ElementNode, ExpressionNode, ExpressionPropNameNode,
    FervidAtom, JsChildNode, Property, SimpleExpressionPropNameNode, StrOrExpr, VBindDirective,
    VueImports, fervid_atom,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        BinExpr, BinaryOp, CallExpr, Callee, ComputedPropName, Expr, ExprOrSpread, IdentName, Lit,
        ParenExpr, PropName, Str,
    },
};

use crate::{
    TransformSfcContext,
    template::{directive_transforms::DirectiveTransformResult, utils::to_camel_case},
};

const ATTR_MARKER: char = '^';
const PROP_MARKER: char = '.';

// Adapted from https://github.com/vuejs/core/blob/d2c458be2542a628878cbfbdfebcb53b65d3e9f3/packages/compiler-core/src/transforms/vBind.ts#L12
pub fn transform_v_bind(
    ctx: &mut TransformSfcContext,
    v_bind: &VBindDirective,
    _node: &ElementNode,
    ssr: bool,
) -> Option<DirectiveTransformResult> {
    let span = v_bind.span;

    // Note: Empty value expressions are already handled by parser

    let Some(ref argument) = v_bind.argument else {
        // v-bind without arg is handled directly in transform_element.rs due to its affecting
        // codegen for the entire props object. This transform here is only for v-bind
        // *with* args.
        return None;
    };

    let needs_prop_or_attr_modifiers = !ssr && (v_bind.is_attr || v_bind.is_prop);

    let key = match argument {
        StrOrExpr::Str(str_arg) => {
            let has_modifiers = v_bind.is_camel || needs_prop_or_attr_modifiers;

            let arg = if has_modifiers {
                let mut out = String::with_capacity(str_arg.len() + 2);

                if v_bind.is_attr && !ssr {
                    out.push(ATTR_MARKER);
                }
                if v_bind.is_prop && !ssr {
                    out.push(PROP_MARKER);
                }
                if v_bind.is_camel {
                    to_camel_case(str_arg, &mut out);
                } else {
                    out.push_str(str_arg);
                }

                FervidAtom::from(out)
            } else {
                str_arg.clone()
            };

            ExpressionPropNameNode::SimpleExpression(SimpleExpressionPropNameNode {
                ast: IdentName { sym: arg, span },
                is_static: true,
                const_type: fervid_core::ConstantTypes::CanStringify,
                is_handler_key: false,
            })
        }

        StrOrExpr::Expr(expr) => {
            let mut key_expr = create_dynamic_bind_key_expr(expr);
            if v_bind.is_camel {
                key_expr = camelize_expr(ctx, key_expr);
            }
            if needs_prop_or_attr_modifiers {
                key_expr = inject_prefix_expr(key_expr, v_bind.is_attr, v_bind.is_prop);
            }

            ExpressionPropNameNode::CompoundExpression(CompoundExpressionPropNameNode {
                ast: PropName::Computed(ComputedPropName {
                    span,
                    expr: key_expr,
                }),
                is_handler_key: false,
            })
        }
    };

    let prop = Property {
        key,
        value: JsChildNode::ExpressionNode(Box::new(ExpressionNode::from(*(v_bind.value).clone()))),
        span: DUMMY_SP,
    };

    Some(DirectiveTransformResult {
        need_runtime: false,
        props: vec![prop],
    })
}

fn create_dynamic_bind_key_expr(expr: &Expr) -> Box<Expr> {
    // (expr) || ""
    Box::new(Expr::Bin(BinExpr {
        span: DUMMY_SP,
        op: BinaryOp::LogicalOr,
        left: Box::new(Expr::Paren(ParenExpr {
            span: DUMMY_SP,
            expr: Box::new(expr.to_owned()),
        })),
        right: Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: fervid_atom!(""),
            raw: None,
        }))),
    }))
}

fn camelize_expr(ctx: &mut TransformSfcContext, expr: Box<Expr>) -> Box<Expr> {
    let camelize: Box<Expr> = ctx
        .bindings_helper
        .helper(VueImports::Camelize)
        .as_atom()
        .into();

    Box::new(Expr::Call(CallExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        callee: Callee::Expr(camelize),
        args: vec![ExprOrSpread { spread: None, expr }],
        type_args: None,
    }))
}

fn inject_prefix_expr(expr: Box<Expr>, is_attr: bool, is_prop: bool) -> Box<Expr> {
    let mut prefix = String::with_capacity(2);
    if is_attr {
        prefix.push(ATTR_MARKER);
    }
    if is_prop {
        prefix.push(PROP_MARKER);
    }

    Box::new(Expr::Bin(BinExpr {
        span: DUMMY_SP,
        op: BinaryOp::Add,
        left: Box::new(Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: FervidAtom::from(prefix),
            raw: None,
        }))),
        right: Box::new(Expr::Paren(ParenExpr {
            span: DUMMY_SP,
            expr,
        })),
    }))
}
