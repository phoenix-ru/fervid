use swc_core::{ecma::{visit::{VisitMut, VisitMutWith}, ast::{PropOrSpread, Prop, KeyValueProp, PropName, Expr, MemberExpr, Ident, MemberProp}, atoms::JsWord}, common::BytePos};
use swc_ecma_parser::{lexer::Lexer, Syntax, StringInput, Parser};

// use crate::analyzer::scope::ScopeHelper;

#[derive(Debug, Default)]
pub struct MockScopeHelper;
pub enum MockScope {
    Yes,
    No
}

/// Dummy scope helper implementation. Delete when real implementation is done
impl MockScopeHelper {
    fn find_scope_of_variable(&self, _current_scope: u32, symbol: &JsWord) -> MockScope {
        match symbol.as_ref() {
            "console" | "undefined" => MockScope::No,
            _ => MockScope::Yes
        }
    }
}

impl MockScope {
    fn get_prefix(&self) -> Option<JsWord> {
        match self {
            MockScope::Yes => Some(JsWord::from("_ctx")),
            MockScope::No => None,
        }
    }
}

pub type ScopeHelper = MockScopeHelper;

pub fn transform_scoped(expr: &str, scope_helper: &ScopeHelper, scope_to_use: u32) -> Option<(Box<Expr>, bool)> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(expr, BytePos(0), BytePos(1000)),
        None
    );

    let mut parser = Parser::new_from(lexer);

    // TODO The use of `parse_expr` vs `parse_stmt` matters
    // For v-for it may be best to use `parse_stmt`, but for `v-slot` you need to use `parse_expr`

    let Ok(mut parsed) = parser.parse_expr() else { return None };

    // Create and invoke the visitor
    let mut visitor = TransformVisitor {
        current_scope: scope_to_use,
        scope_helper,
        has_js_bindings: false
    };
    parsed.visit_mut_with(&mut visitor);

    Some((parsed, visitor.has_js_bindings))
}

struct TransformVisitor <'s> {
    current_scope: u32,
    scope_helper: &'s ScopeHelper,
    has_js_bindings: bool
}

impl <'s> VisitMut for TransformVisitor <'s> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        match n {
            Expr::Ident(ident_expr) => {
                let symbol = &ident_expr.sym;
                let scope = self.scope_helper.find_scope_of_variable(self.current_scope, symbol);

                // Get the prefix which fits the scope (e.g. `_ctx.` for unknown scopes, `$setup.` for setup scope)
                if let Some(prefix) = scope.get_prefix() {
                    *n = Expr::Member(MemberExpr {
                        span: ident_expr.span,
                        obj: Box::new(Expr::Ident(Ident {
                            span: ident_expr.span,
                            sym: prefix,
                            optional: false,
                        })),
                        prop: MemberProp::Ident(ident_expr.to_owned()),
                    });
                    self.has_js_bindings = true;
                }
            }

            _ => n.visit_mut_children_with(self)
        }
    }

    // fn visit_mut_ident(&mut self, n: &mut swc_core::ecma::ast::Ident) {
    //     let symbol = &n.sym;
    //     let scope = self.scope_helper.find_scope_of_variable(self.current_scope, symbol);

    //     let prefix = scope.get_prefix();
    //     if prefix.len() > 0 {
    //         let mut new_symbol = String::with_capacity(symbol.len() + prefix.len());
    //         new_symbol.push_str(prefix);
    //         new_symbol.push_str(&symbol);
    //         n.sym = new_symbol.into();
    //     }
    // }

    fn visit_mut_member_expr(&mut self, n: &mut swc_core::ecma::ast::MemberExpr) {
        if n.obj.is_ident() {
            n.obj.visit_mut_with(self)
        } else {
            n.visit_mut_children_with(self);
        }
    }

    fn visit_mut_object_lit(&mut self, n: &mut swc_core::ecma::ast::ObjectLit) {
        for prop in n.props.iter_mut() {
            match prop {
                PropOrSpread::Prop(ref mut prop) => {
                    // For shorthand, expand it and visit the value part
                    if let Some(shorthand) = prop.as_mut_shorthand() {
                        let prop_name = PropName::Ident(shorthand.to_owned());

                        let mut value_expr = Expr::Ident(shorthand.to_owned());
                        value_expr.visit_mut_with(self);

                        *prop = Prop::KeyValue(KeyValueProp {
                            key: prop_name,
                            value: Box::new(value_expr)
                        }).into();
                        self.has_js_bindings = true;
                    } else if let Some(keyvalue) = prop.as_mut_key_value() {
                        keyvalue.value.visit_mut_with(self);
                    }
                },

                PropOrSpread::Spread(ref mut spread) => {
                    spread.visit_mut_with(self);
                }
            }
        }
    }
}
