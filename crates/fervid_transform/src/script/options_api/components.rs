use swc_core::ecma::ast::ObjectLit;

use crate::{script::utils::collect_obj_fields, structs::ScriptLegacyVars};

/// Collects the components in form `{ Foo, BarBaz, Qux: ComponentQux }`
///
/// https://vuejs.org/api/options-misc.html#components
#[inline]
pub fn collect_components_object(obj_lit: &ObjectLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_obj_fields(obj_lit, &mut script_legacy_vars.components)
}
