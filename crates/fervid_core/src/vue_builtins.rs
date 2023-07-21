use phf::phf_map;

use crate::BuiltinType;

pub static VUE_BUILTINS: phf::Map<&'static str, BuiltinType> = phf_map! {
    "keep-alive" => BuiltinType::KeepAlive,
    "KeepAlive" => BuiltinType::KeepAlive,
    "slot" => BuiltinType::Slot,
    "Slot" => BuiltinType::Slot,
    "suspense" => BuiltinType::Suspense,
    "Suspense" => BuiltinType::Suspense,
    "teleport" => BuiltinType::Teleport,
    "Teleport" => BuiltinType::Teleport,
    "transition" => BuiltinType::Transition,
    "Transition" => BuiltinType::Transition,
    "transition-group" => BuiltinType::TransitionGroup,
    "TransitionGroup" => BuiltinType::TransitionGroup,
};
