mod all_html_tags;
mod bindings;
mod sfc;
mod structs;
mod template;
mod vue_builtins;
mod vue_imports;

pub use all_html_tags::is_html_tag;
pub use bindings::*;
pub use sfc::*;
pub use structs::*;
pub use template::is_from_default_slot;
pub use vue_builtins::VUE_BUILTINS;
pub use vue_imports::{VueImports, VueImportsSet};
