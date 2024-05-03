use crate::OptionsApiBindings;
use swc_core::ecma::ast::{ObjectLit, ArrayLit};

use crate::script::utils::{collect_obj_fields, collect_string_arr};

/// Collects emits defined in object syntax, e.g. `{ foo: null, bar: (v) => !!v }`
///
/// https://vuejs.org/api/options-state.html#emits
#[inline]
pub fn collect_emits_bindings_object(obj_lit: &ObjectLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.emits)
}

/// Collects an array of emits defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/api/options-state.html#emits
#[inline]
pub fn collect_emits_bindings_array(arr: &ArrayLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_string_arr(arr, &mut options_api_bindings.emits)
}
