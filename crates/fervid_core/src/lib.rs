mod all_html_tags;
mod sfc;
mod structs;
mod template;
mod vue_builtins;

pub use all_html_tags::is_html_tag;
pub use sfc::*;
pub use structs::*;
pub use template::is_from_default_slot;
pub use vue_builtins::VUE_BUILTINS;
