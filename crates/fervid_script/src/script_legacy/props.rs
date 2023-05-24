use swc_core::ecma::ast::{ObjectLit, ArrayLit};

use crate::{script_legacy::ScriptLegacyVars, common::utils::{collect_obj_fields, collect_string_arr}};

/// Collects props defined in object syntax, e.g. `{ foo: { type: String }, bar: { type: Number } }`
///
/// https://vuejs.org/guide/components/props.html
#[inline]
pub fn collect_prop_bindings_object(obj_lit: &ObjectLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_obj_fields(obj_lit, &mut script_legacy_vars.props)
}

/// Collects an array of props defined as `string[]`, e.g. `['foo', 'bar', 'baz']`
///
/// https://vuejs.org/guide/components/props.html
#[inline]
pub fn collect_prop_bindings_array(arr: &ArrayLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_string_arr(arr, &mut script_legacy_vars.props)
}
