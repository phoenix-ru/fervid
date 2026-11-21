use fervid_core::{ComponentBinding, FervidAtom, IntoIdent, VueImports};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        BindingIdent, CallExpr, Callee, Expr, ExprOrSpread, Ident, Lit, Pat, Str, VarDeclarator,
    },
};

use crate::CodegenContext;

impl CodegenContext {
    pub fn generate_component_resolves(&mut self) -> Vec<VarDeclarator> {
        let mut result = Vec::new();

        if self.bindings_helper.components.is_empty() {
            return result;
        }

        // We are borrowing `self` later so we cannot use this on demand
        let resolve_component_ident = VueImports::ResolveComponent.as_atom().into_ident();

        // We need sorted entries for stable output.
        // Entries are sorted by a component name (first element of tuple in hashmap entry)
        let mut sorted_components: Vec<(&FervidAtom, &Ident, bool)> = self
            .bindings_helper
            .components
            .iter()
            .filter_map(
                |(component_name, component_resolution)| match component_resolution {
                    ComponentBinding::RuntimeResolved(ident, is_self_reference) => {
                        Some((component_name, ident.as_ref(), *is_self_reference))
                    }
                    _ => None,
                },
            )
            .collect();

        sorted_components.sort_by(|a, b| a.0.cmp(b.0));

        // Key is a component as used in template, value is the assigned Js identifier
        for (component_name, component_identifier, is_self_reference) in sorted_components.iter() {
            let mut args = Vec::with_capacity(1 + *is_self_reference as usize);
            args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: (*component_name).to_owned(),
                    raw: None,
                }))),
            });
            if *is_self_reference {
                args.push(ExprOrSpread {
                    spread: None,
                    expr: Box::new(true.into()),
                });
            }

            // _component_ident_name = resolveComponent("component-name")
            result.push(VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: (*component_identifier).to_owned(),
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    callee: Callee::Expr(Box::new(Expr::Ident(resolve_component_ident.to_owned()))),
                    args,
                    type_args: None,
                }))),
                definite: false,
            });
        }

        // If we had a component, register the import
        if !sorted_components.is_empty() {
            self.add_to_imports(VueImports::ResolveComponent);
        }

        result
    }
}
