use fervid_core::OptionsApiBindings;
use swc_core::ecma::ast::ObjectLit;

use crate::script::utils::collect_obj_fields;

/// Collects the computed bindings in form `{ foo() { return this.bar }, baz: () => 'qux' }`
///
/// https://vuejs.org/api/options-state.html#computed
#[inline]
pub fn collect_computed_object(obj_lit: &ObjectLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.computed)
}
