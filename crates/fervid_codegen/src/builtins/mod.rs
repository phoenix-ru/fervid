use fervid_core::{ElementNode, BuiltinType};
use swc_core::ecma::ast::Expr;

use crate::CodegenContext;

mod common;
mod component;
mod keepalive;
mod slot;
mod suspense;
mod teleport;
mod transition;
mod transition_group;

impl CodegenContext {
    pub fn generate_builtin(&mut self, element_node: &ElementNode, builtin_type: BuiltinType) -> Expr {
        match builtin_type {
            BuiltinType::Component => self.generate_component_builtin(element_node),
            BuiltinType::KeepAlive => self.generate_keepalive(element_node),
            BuiltinType::Slot => self.generate_slot(element_node),
            BuiltinType::Suspense => self.generate_suspense(element_node),
            BuiltinType::Teleport => self.generate_teleport(element_node),
            BuiltinType::Transition => self.generate_transition(element_node),
            BuiltinType::TransitionGroup => self.generate_transition_group(element_node),
        }
    }
}
