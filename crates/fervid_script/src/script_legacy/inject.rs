use swc_core::ecma::ast::{ObjectLit, ArrayLit};

use crate::{script_legacy::ScriptLegacyVars, common::utils::{collect_obj_fields, collect_string_arr}};

/// Collects injects defined in object syntax, e.g. `{ foo: 'foo', bar: { from: 'baz' } }`
///
/// https://vuejs.org/api/options-composition.html#inject
#[inline]
pub fn collect_inject_bindings_object(obj_lit: &ObjectLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_obj_fields(obj_lit, &mut script_legacy_vars.inject)
}

/// Collects an array of injects defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/api/options-composition.html#inject
#[inline]
pub fn collect_inject_bindings_array(arr: &ArrayLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_string_arr(arr, &mut script_legacy_vars.inject)
}
