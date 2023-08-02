use swc_core::ecma::{ast::Ident, visit::{Visit, VisitWith}};

use crate::structs::TemplateScope;

/// Polyfill for variable collection before the 
pub fn collect_variables(root: &impl VisitWith<IdentifierVisitor>, scope: &mut TemplateScope) {
    let mut visitor = IdentifierVisitor { collected: vec![] };

    root.visit_with(&mut visitor);

    scope.variables.reserve(visitor.collected.len());
    for collected in visitor.collected {
        scope.variables.push(collected.sym)
    }
}

pub struct IdentifierVisitor {
    collected: Vec<Ident>,
}

impl Visit for IdentifierVisitor {
    fn visit_ident(&mut self, n: &swc_core::ecma::ast::Ident) {
        self.collected.push(n.to_owned());
    }

    fn visit_object_lit(&mut self, n: &swc_core::ecma::ast::ObjectLit) {
        self.collected.reserve(n.props.len());

        for prop in n.props.iter() {
            let swc_core::ecma::ast::PropOrSpread::Prop(prop) = prop else {
                continue;
            };

            // This is shorthand `a` in `{ a }`
            let shorthand = prop.as_shorthand();
            if let Some(ident) = shorthand {
                self.collected.push(ident.to_owned());
                continue;
            }

            // This is key-value `a: b` in `{ a: b }`
            let Some(keyvalue) = prop.as_key_value() else { continue };

            // We only support renaming things (therefore value must be an identifier)
            let Some(value) = keyvalue.value.as_ident() else { continue };
            self.collected.push(value.to_owned());
        }
    }
}
