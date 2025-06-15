use crate::OptionsApiBindings;
use swc_core::ecma::ast::ObjectLit;

use crate::script::utils::collect_obj_fields;

/// Collects the components in form `{ Foo, BarBaz, Qux: ComponentQux }`
///
/// https://vuejs.org/api/options-misc.html#components
#[inline]
pub fn collect_components_object(
    obj_lit: &ObjectLit,
    options_api_bindings: &mut OptionsApiBindings,
) {
    collect_obj_fields(obj_lit, &mut options_api_bindings.components)
}
