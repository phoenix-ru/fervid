use swc_core::ecma::ast::{BlockStmt, Expr};

use crate::{structs::{BindingTypes, ScriptLegacyVars, SetupBinding}, common::utils::{collect_block_stmt_return_fields, unroll_paren_seq, collect_obj_fields}};

/// Collects all the bindings from `setup`, e.g. `setup() { return { foo: 'bar', baz: 42 } }`
///
/// https://vuejs.org/api/composition-api-setup.html
#[inline]
pub fn collect_setup_bindings_block_stmt(block_stmt: &BlockStmt, script_legacy_vars: &mut ScriptLegacyVars) {
    // TODO Implement the algorithm

    let mut tmp = Vec::new();
    collect_block_stmt_return_fields(block_stmt, &mut tmp);

    script_legacy_vars.setup.extend(
        tmp.into_iter().map(|word| SetupBinding(word, BindingTypes::SetupMaybeRef))
    );
}

/// Collects all the bindings from `setup` expression, e.g. `setup: () => ({ foo: 'bar' })`
///
/// https://vuejs.org/api/composition-api-setup.html
#[inline]
pub fn collect_setup_bindings_expr(expr: &Expr, script_legacy_vars: &mut ScriptLegacyVars) {
    let expr = unroll_paren_seq(expr);

    // TODO The actual algorithm is much more complicated

    let Expr::Object(ref obj_lit) = *expr else {
        return;
    };

    let mut tmp = Vec::new();
    collect_obj_fields(obj_lit, &mut tmp);

    script_legacy_vars.setup.extend(
        tmp.into_iter().map(|word| SetupBinding(word, BindingTypes::SetupMaybeRef))
    );
}
