use swc_core::ecma::ast::ObjectLit;

use crate::{script_legacy::ScriptLegacyVars, common::utils::collect_obj_fields};

/// Collects the directive bindings in form `{ foo: { /*...*/ }, bar }`
///
/// https://vuejs.org/api/options-misc.html#directives
#[inline]
pub fn collect_directives_object(obj_lit: &ObjectLit, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_obj_fields(obj_lit, &mut script_legacy_vars.directives)
}
