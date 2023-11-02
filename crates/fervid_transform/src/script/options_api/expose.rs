use fervid_core::OptionsApiBindings;
use swc_core::ecma::ast::ArrayLit;

use crate::script::utils::collect_string_arr;

/// Collects an array of exposes defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/api/options-state.html#expose
#[inline]
pub fn collect_expose_bindings_array(arr: &ArrayLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_string_arr(arr, &mut options_api_bindings.expose)
}
