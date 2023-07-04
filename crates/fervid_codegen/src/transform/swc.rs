use swc_core::{
    common::BytePos,
    ecma::{
        ast::{Expr, Ident, KeyValueProp, MemberExpr, MemberProp, Prop, PropName, PropOrSpread},
        visit::{VisitMut, VisitMutWith},
    },
};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};

use crate::context::{get_prefix, ScopeHelper};

pub fn transform_scoped(
    expr: &str,
    scope_helper: &ScopeHelper,
    scope_to_use: u32,
) -> Option<(Box<Expr>, bool)> {
    let lexer = Lexer::new(
        // We want to parse ecmascript
        Syntax::Es(Default::default()),
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(expr, BytePos(0), BytePos(1000)),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    let Ok(mut parsed) = parser.parse_expr() else { return None };

    // Create and invoke the visitor
    let mut visitor = TransformVisitor {
        current_scope: scope_to_use,
        scope_helper,
        has_js_bindings: false,
        is_inline: scope_helper.is_inline
    };
    parsed.visit_mut_with(&mut visitor);

    Some((parsed, visitor.has_js_bindings))
}

struct TransformVisitor<'s> {
    current_scope: u32,
    scope_helper: &'s ScopeHelper,
    has_js_bindings: bool,
    is_inline: bool,
}

impl<'s> VisitMut for TransformVisitor<'s> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        match n {
            Expr::Ident(ident_expr) => {
                let symbol = &ident_expr.sym;
                let binding_type = self
                    .scope_helper
                    .get_var_binding_type(self.current_scope, symbol);

                // TODO The logic for setup variables actually differs quite significantly
                // https://play.vuejs.org/#eNp9UU1rwzAM/SvCl25QEkZvIRTa0cN22Mq6oy8hUVJ3iW380QWC//tkh2Y7jN6k956kJ2liO62zq0dWsNLWRmgHFp3XWy7FoJVxMMEOArRGDbDK8v2KywZbIfFolLYPE5cArVIFnJwRsuMyPHJZ5nMv6kKJw0H3lUPKAMrz03aaYgmEQAE1D2VOYKxalGzNnK2VbEWXXaySZC9N4qxWgxY9mnfthJKWswISE7mq79X3a8Kc8bi+4fUZ669/8IsdI8bZ0aBFc0XOFs5VpkM304fTG44UL+SgGt+T+g75gVb1PnqcZXsvG7L9R5fcvqQj0+E+7WF0KO1tqWg0KkPSc0Y/er6z+q/dTbZJdfQJFn4A+DKelw==

                // Get the prefix which fits the scope (e.g. `_ctx.` for unknown scopes, `$setup.` for setup scope)
                if let Some(prefix) = get_prefix(&binding_type, self.is_inline) {
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

            _ => n.visit_mut_children_with(self),
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
                            value: Box::new(value_expr),
                        })
                        .into();
                        self.has_js_bindings = true;
                    } else if let Some(keyvalue) = prop.as_mut_key_value() {
                        keyvalue.value.visit_mut_with(self);
                    }
                }

                PropOrSpread::Spread(ref mut spread) => {
                    spread.visit_mut_with(self);
                }
            }
        }
    }
}
