use swc_core::ecma::ast::ObjectLit;

use super::{ScriptLegacyVars, utils::collect_obj_fields};

/// Collects the computed bindings in form `{ foo() { return this.bar }, baz: () => 'qux' }`
///
/// https://vuejs.org/api/options-state.html#computed
#[inline]
pub fn collect_computed_object(obj_lit: &ObjectLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_obj_fields(obj_lit, &mut script_legacy_vars.computed)
}
