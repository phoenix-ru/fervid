use swc_common::{BytePos, sync::Lrc, SourceMap};
use swc_core::ecma::{visit::{VisitMut, VisitMutWith}, ast::{PropOrSpread, Prop, KeyValueProp, PropName, Expr}};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};
use swc_ecma_parser::{lexer::Lexer, Syntax, StringInput, Parser};

use crate::analyzer::scope::ScopeHelper;

pub fn transform_scoped(expr: &str, scope_helper: &ScopeHelper, scope_to_use: u32) -> Option<String> {
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
        scope_helper
    };
    parsed.visit_mut_with(&mut visitor);

    // Emitting the result requires some setup with SWC
    let cm: Lrc<SourceMap> = Default::default();
    let mut buff: Vec<u8> = Vec::with_capacity(expr.len() * 2);
    let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

    let mut emitter = Emitter {
        cfg: swc_ecma_codegen::Config {
            target: Default::default(),
            ascii_only: false,
            minify: true,
            omit_last_semi: false
        },
        comments: None,
        wr: writer,
        cm
    };

    let _ = parsed.emit_with(&mut emitter);

    Some(String::from_utf8(buff).unwrap())
}

struct TransformVisitor <'s> {
    current_scope: u32,
    scope_helper: &'s ScopeHelper
}

impl <'s> VisitMut for TransformVisitor <'s> {
    fn visit_mut_ident(&mut self, n: &mut swc_core::ecma::ast::Ident) {
        let symbol = &n.sym;
        let scope = self.scope_helper.find_scope_of_variable(self.current_scope, &symbol);

        // Get the prefix which fits the scope (e.g. `_ctx.` for unknown scopes, `$setup.` for setup scope)
        let prefix = scope.get_prefix();
        if prefix.len() > 0 {
            let mut new_symbol = String::with_capacity(symbol.len() + prefix.len());
            new_symbol.push_str(prefix);
            new_symbol.push_str(&symbol);
            n.sym = new_symbol.into();
        }
    }

    fn visit_mut_member_expr(&mut self, n: &mut swc_core::ecma::ast::MemberExpr) {
        if let Some(old_ident) = n.obj.as_mut_ident() {
            old_ident.visit_mut_with(self);
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
                        let mut value = shorthand.to_owned();
                        value.visit_mut_with(self);

                        *prop = Prop::KeyValue(KeyValueProp {
                            key: prop_name,
                            value: Box::new(Expr::Ident(value))
                        }).into();
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
