use crate::OptionsApiBindings;
use swc_core::ecma::ast::{Expr, FunctionBody};

use crate::script::utils::{
    collect_function_body_return_fields, collect_obj_fields, unroll_paren_seq,
};

/// Collects variables from `data`.
/// Supports `data() {}`, `data: function() {}` and `data: () => {}`
///
/// https://vuejs.org/api/options-state.html#data
#[inline]
pub fn collect_data_bindings_function_body(
    function_body: &FunctionBody,
    options_api_bindings: &mut OptionsApiBindings,
) {
    collect_function_body_return_fields(function_body, &mut options_api_bindings.data)
}

/// Collects variables from `data: () => ({ foo: 'bar' })`
///
/// https://vuejs.org/api/options-state.html#data
#[inline]
pub fn collect_data_bindings_expr(expr: &Expr, options_api_bindings: &mut OptionsApiBindings) {
    let expr = unroll_paren_seq(expr);

    let Expr::Object(ref obj_lit) = *expr else {
        return;
    };

    collect_obj_fields(obj_lit, &mut options_api_bindings.data)
}
