use crate::OptionsApiBindings;
use swc_core::ecma::ast::{ArrayLit, ObjectLit};

use crate::script::utils::{collect_obj_fields, collect_string_arr};

/// Collects props defined in object syntax, e.g. `{ foo: { type: String }, bar: { type: Number } }`
///
/// https://vuejs.org/guide/components/props.html
#[inline]
pub fn collect_prop_bindings_object(
    obj_lit: &ObjectLit,
    options_api_bindings: &mut OptionsApiBindings,
) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.props)
}

/// Collects an array of props defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/guide/components/props.html
#[inline]
pub fn collect_prop_bindings_array(arr: &ArrayLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_string_arr(arr, &mut options_api_bindings.props)
}
