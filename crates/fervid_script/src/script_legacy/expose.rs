use swc_core::ecma::ast::ArrayLit;

use crate::{script_legacy::ScriptLegacyVars, common::utils::collect_string_arr};

/// Collects an array of exposes defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/api/options-state.html#expose
#[inline]
pub fn collect_expose_bindings_array(arr: &ArrayLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_string_arr(arr, &mut script_legacy_vars.expose)
}
