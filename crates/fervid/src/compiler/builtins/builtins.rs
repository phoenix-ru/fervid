use fervid_core::ElementNode;

use crate::compiler::codegen::CodegenContext;

#[derive(Debug)]
pub enum VueBuiltin {
    KeepAlive,
    Slot,
    Suspense,
    Teleport,
    Transition,
    TransitionGroup,
}

impl CodegenContext<'_> {
    pub fn is_builtin(element_node: &ElementNode) -> Option<VueBuiltin> {
        match element_node.starting_tag.tag_name {
            "keep-alive" | "KeepAlive" => Some(VueBuiltin::KeepAlive),
            "slot" | "Slot" => Some(VueBuiltin::Slot),
            "suspense" | "Suspense" => Some(VueBuiltin::Suspense),
            "teleport" | "Teleport" => Some(VueBuiltin::Teleport),
            "transition" | "Transition" => Some(VueBuiltin::Transition),
            "transition-group" | "TransitionGroup" => Some(VueBuiltin::TransitionGroup),
            _ => None
        }
    }

    pub fn compile_builtin(
        &mut self,
        buf: &mut String,
        element_node: &ElementNode,
        builtin_type: VueBuiltin,
    ) {
        // format!()
        match builtin_type {
            VueBuiltin::Slot => self.compile_slot(buf, element_node),
            _ => todo!("Compiling this built-in is unsupported yet: {:?}", builtin_type)
        }
    }
}
