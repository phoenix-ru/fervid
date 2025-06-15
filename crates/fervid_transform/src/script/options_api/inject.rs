use crate::OptionsApiBindings;
use swc_core::ecma::ast::{ArrayLit, ObjectLit};

use crate::script::utils::{collect_obj_fields, collect_string_arr};

/// Collects injects defined in object syntax, e.g. `{ foo: 'foo', bar: { from: 'baz' } }`
///
/// https://vuejs.org/api/options-composition.html#inject
#[inline]
pub fn collect_inject_bindings_object(
    obj_lit: &ObjectLit,
    options_api_bindings: &mut OptionsApiBindings,
) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.inject)
}

/// Collects an array of injects defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/api/options-composition.html#inject
#[inline]
pub fn collect_inject_bindings_array(
    arr: &ArrayLit,
    options_api_bindings: &mut OptionsApiBindings,
) {
    collect_string_arr(arr, &mut options_api_bindings.inject)
}
