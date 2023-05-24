use swc_core::ecma::ast::ObjectLit;

use crate::{
    script_legacy::ScriptLegacyVars,
    common::utils::collect_obj_fields
};

/// Collects the methods bindings in form `{ foo() { this.bar = 'bar' }, baz: () => { console.log('qux') } }`
///
/// https://vuejs.org/api/options-state.html#methods
#[inline]
pub fn collect_methods_object(obj_lit: &ObjectLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_obj_fields(obj_lit, &mut script_legacy_vars.methods)
}
