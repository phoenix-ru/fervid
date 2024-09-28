use fervid_core::{fervid_atom, AttributeOrBinding, ElementNode, IntoIdent};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::ast::{Expr, KeyValueProp, Lit, Number, ObjectLit, Prop, PropName, PropOrSpread},
};

use crate::CodegenContext;

impl CodegenContext {
    /// Generates attributes if any are present, returns `None` otherwise
    pub(crate) fn generate_builtin_attrs(
        &mut self,
        attributes: &[AttributeOrBinding],
        span: Span,
    ) -> Option<Expr> {
        if attributes.len() != 0 {
            let mut attrs = Vec::with_capacity(attributes.len());
            self.generate_attributes(&attributes, &mut attrs);
            Some(Expr::Object(ObjectLit { span, props: attrs }))
        } else {
            None
        }
    }

    /// Generates the slots expression for builtins.
    ///
    /// Additionally adds `_: 1` to the slots object.
    pub(crate) fn generate_builtin_slots(&mut self, element_node: &ElementNode) -> Option<Expr> {
        let mut slots = self.generate_component_children(element_node);
        if let Some(Expr::Object(ref mut obj)) = slots {
            obj.props
                .push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(fervid_atom!("_").into_ident().into()),
                    value: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value: 1.0,
                        raw: None,
                    }))),
                }))));
        }

        slots
    }
}
