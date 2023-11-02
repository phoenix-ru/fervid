use fervid_core::{BindingTypes, OptionsApiBindings, SetupBinding};
use swc_core::ecma::ast::{BlockStmt, Expr};

use crate::script::utils::{
    collect_block_stmt_return_fields, collect_obj_fields, unroll_paren_seq,
};

/// Collects all the bindings from `setup`, e.g. `setup() { return { foo: 'bar', baz: 42 } }`
///
/// https://vuejs.org/api/composition-api-setup.html
#[inline]
pub fn collect_setup_bindings_block_stmt(
    block_stmt: &BlockStmt,
    options_api_bindings: &mut OptionsApiBindings,
) {
    // TODO Implement the algorithm
    // But the current Vue SFC compiler is doing the same

    let mut tmp = Vec::new();
    collect_block_stmt_return_fields(block_stmt, &mut tmp);

    options_api_bindings.setup.extend(
        tmp.into_iter()
            .map(|word| SetupBinding(word, BindingTypes::SetupMaybeRef)),
    );
}

/// Collects all the bindings from `setup` expression, e.g. `setup: () => ({ foo: 'bar' })`
///
/// https://vuejs.org/api/composition-api-setup.html
#[inline]
pub fn collect_setup_bindings_expr(expr: &Expr, options_api_bindings: &mut OptionsApiBindings) {
    let expr = unroll_paren_seq(expr);

    // TODO The actual algorithm is much more complicated
    // But the current Vue SFC compiler is doing the same

    let Expr::Object(ref obj_lit) = *expr else {
        return;
    };

    let mut tmp = Vec::new();
    collect_obj_fields(obj_lit, &mut tmp);

    options_api_bindings.setup.extend(
        tmp.into_iter()
            .map(|word| SetupBinding(word, BindingTypes::SetupMaybeRef)),
    );
}
