use crate::OptionsApiBindings;
use swc_core::ecma::ast::ObjectLit;

use crate::script::utils::collect_obj_fields;

/// Collects the directive bindings in form `{ foo: { /*...*/ }, bar }`
///
/// https://vuejs.org/api/options-misc.html#directives
#[inline]
pub fn collect_directives_object(obj_lit: &ObjectLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.directives)
}
