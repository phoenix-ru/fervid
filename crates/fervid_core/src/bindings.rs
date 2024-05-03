use swc_core::ecma::ast::{Expr, Ident};

use crate::BuiltinType;

/// A binding of a component which was found in the template
#[derive(Debug, Default)]
pub enum ComponentBinding {
    /// Component was resolved to something specific, e.g. an import.
    /// The contained `Expr` is for the resolved value (usually identifier or `unref(ident)`)
    Resolved(Box<Expr>),

    /// Component must be resolved in runtime, i.e. using `resolveComponent` call.
    /// The contained value is an identifier,
    /// e.g. `_component_custom` in `const _component_custom = resolveComponent('custom')`
    RuntimeResolved(Box<Ident>),

    /// Component was not resolved and would need to be
    /// either transformed (this is default from parser) or ignored
    #[default]
    Unresolved,

    /// Component was resolved to be a Vue built-in
    Builtin(BuiltinType),
}

/// A binding of a directive which was found in the template
#[derive(Debug, Default)]
pub enum CustomDirectiveBinding {
    /// Custom directive was resolved,
    /// usually to an identifier which has a form `vCustomDirective` (corresponds to `v-custom-directive`).
    Resolved(Box<Expr>),

    /// Custom directive must be resolved in runtime, i.e. using `resolveDirective` call.
    /// The contained value is an identifier,
    /// e.g. `_directive_custom` in `const _directive_custom = resolveDirective('custom')`
    RuntimeResolved(Box<Ident>),

    /// Custom directive was not resolved and would need to be resolved in runtime
    #[default]
    Unresolved,
}
