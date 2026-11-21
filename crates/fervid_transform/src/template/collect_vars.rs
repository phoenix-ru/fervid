use swc_core::ecma::{
    ast::{Ident, ObjectPatProp, Pat},
    visit::{Visit, VisitWith},
};

use crate::TemplateScope;

/// Polyfill for variable collection before the
pub fn collect_variables(root: &impl VisitWith<IdentifierVisitor>, scope: &mut TemplateScope) {
    let mut visitor = IdentifierVisitor { collected: vec![] };

    root.visit_with(&mut visitor);

    scope.variables.reserve(visitor.collected.len());
    for collected in visitor.collected {
        scope.variables.push(collected.sym)
    }
}

/// Find such bindings inside a pattern which would become
/// function local variables.
/// For example, `<template v-slot="{ value = outer }">` would collect `value`
/// since it is the one which becomes available to `template` children,
/// while `outer` is its default value which needs to be transformed.
/// In that example, `({ value = _ctx.outer }) => /* value is used as-is */` is the intended result.
pub fn collect_pattern_bindings(root: &Pat, scope: &mut TemplateScope) {
    let mut collected = vec![];

    collect_pattern_binding_idents(root, &mut collected);

    scope.variables.reserve(collected.len());
    for collected in collected {
        scope.variables.push(collected.sym)
    }
}

fn collect_pattern_binding_idents(root: &Pat, collected: &mut Vec<Ident>) {
    match root {
        Pat::Ident(binding) => collected.push(binding.id.to_owned()),
        Pat::Array(array) => {
            collected.reserve(array.elems.len());
            for element in array.elems.iter().flatten() {
                collect_pattern_binding_idents(element, collected);
            }
        }
        Pat::Rest(rest) => collect_pattern_binding_idents(&rest.arg, collected),
        Pat::Object(object) => {
            collected.reserve(object.props.len());
            for prop in &object.props {
                match prop {
                    ObjectPatProp::KeyValue(key_value) => {
                        collect_pattern_binding_idents(&key_value.value, collected);
                    }
                    ObjectPatProp::Assign(assign) => collected.push(assign.key.id.to_owned()),
                    ObjectPatProp::Rest(rest) => {
                        collect_pattern_binding_idents(&rest.arg, collected);
                    }
                }
            }
        }
        Pat::Assign(assign) => collect_pattern_binding_idents(&assign.left, collected),
        Pat::Expr(_) | Pat::Invalid(_) => {}
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
            let Some(keyvalue) = prop.as_key_value() else {
                continue;
            };

            // We only support renaming things (therefore value must be an identifier)
            let Some(value) = keyvalue.value.as_ident() else {
                continue;
            };
            self.collected.push(value.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use swc_core::{
        common::{BytePos, Span},
        ecma::ast::{EsVersion, Pat},
    };
    use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

    use super::{collect_pattern_bindings, collect_variables};
    use crate::{TemplateScope, test_utils::ts};

    fn collect_from_pat(raw: &str) -> Vec<String> {
        let pat = parse_pat(raw);
        collect(&pat)
    }

    fn collect_bindings_from_pat(raw: &str) -> Vec<String> {
        let pat = parse_pat(raw);
        let mut scope = TemplateScope {
            variables: Default::default(),
            parent: 0,
        };

        collect_pattern_bindings(&pat, &mut scope);

        scope
            .variables
            .into_iter()
            .map(|variable| variable.to_string())
            .collect()
    }

    fn collect(
        root: &impl swc_core::ecma::visit::VisitWith<super::IdentifierVisitor>,
    ) -> Vec<String> {
        let mut scope = TemplateScope {
            variables: Default::default(),
            parent: 0,
        };

        collect_variables(root, &mut scope);

        scope
            .variables
            .into_iter()
            .map(|variable| variable.to_string())
            .collect()
    }

    fn parse_pat(raw: &str) -> Pat {
        let span = Span::new(BytePos(0), BytePos(raw.len() as u32));
        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax::default()),
            EsVersion::EsNext,
            StringInput::new(raw, span.lo, span.hi),
            None,
        );

        Parser::new_from(lexer)
            .parse_pat()
            .expect("slot props pattern should parse")
    }

    #[test]
    fn collect_pat_identifier() {
        assert_eq!(collect_from_pat("slotProps"), vec!["slotProps"]);
    }

    #[test]
    fn collect_expr_identifier() {
        assert_eq!(collect(ts("slotProps").as_ref()), vec!["slotProps"]);
    }

    #[test]
    fn collect_pat_object_destructure_rename_current_behavior() {
        assert_eq!(collect_from_pat("{ foo: bar }"), vec!["bar"]);
    }

    #[test]
    fn collect_expr_object_rename_uses_value_identifier_only() {
        assert_eq!(collect(ts("({ foo: bar })").as_ref()), vec!["bar"]);
    }

    #[test]
    fn collect_pat_object_default_current_behavior() {
        assert_eq!(
            collect_from_pat("{ value = outer }"),
            vec!["value", "outer"]
        );
    }

    #[test]
    fn collect_pat_object_computed_key_current_behavior() {
        assert_eq!(collect_from_pat("{ [key]: value }"), vec!["key", "value"]);
    }

    #[test]
    fn collect_pat_rest_and_nested_current_behavior() {
        assert_eq!(
            collect_from_pat("{ foo: { bar }, ...rest }"),
            vec!["bar", "rest"]
        );
    }

    #[test]
    fn collect_pattern_bindings_identifier() {
        assert_eq!(collect_bindings_from_pat("slotProps"), vec!["slotProps"]);
    }

    #[test]
    fn collect_pattern_bindings_object_destructure_rename() {
        assert_eq!(collect_bindings_from_pat("{ foo: bar }"), vec!["bar"]);
    }

    #[test]
    fn collect_pattern_bindings_ignores_default_references() {
        assert_eq!(
            collect_bindings_from_pat("{ value = outer }"),
            vec!["value"]
        );
    }

    #[test]
    fn collect_pattern_bindings_ignores_computed_key_references() {
        assert_eq!(collect_bindings_from_pat("{ [key]: value }"), vec!["value"]);
    }

    #[test]
    fn collect_pattern_bindings_rest_and_nested() {
        assert_eq!(
            collect_bindings_from_pat("{ foo: { bar }, ...rest }"),
            vec!["bar", "rest"]
        );
    }
}
