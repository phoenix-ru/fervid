use crate::OptionsApiBindings;
use swc_core::ecma::ast::ObjectLit;

use crate::script::utils::collect_obj_fields;

/// Collects the methods bindings in form `{ foo() { this.bar = 'bar' }, baz: () => { console.log('qux') } }`
///
/// https://vuejs.org/api/options-state.html#methods
#[inline]
pub fn collect_methods_object(obj_lit: &ObjectLit, options_api_bindings: &mut OptionsApiBindings) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.methods)
}
