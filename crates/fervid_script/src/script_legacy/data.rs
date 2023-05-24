use swc_core::ecma::ast::{BlockStmt, Expr};

use crate::{script_legacy::ScriptLegacyVars, common::utils::{collect_block_stmt_return_fields, unroll_paren_seq, collect_obj_fields}};

/// Collects variables from `data`.
/// Supports `data() {}`, `data: function() {}` and `data: () => {}`
///
/// https://vuejs.org/api/options-state.html#data
#[inline]
pub fn collect_data_bindings_block_stmt(block_stmt: &BlockStmt, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_block_stmt_return_fields(block_stmt, &mut script_legacy_vars.data)
}

/// Collects variables from `data: () => ({ foo: 'bar' })`
///
/// https://vuejs.org/api/options-state.html#data
#[inline]
pub fn collect_data_bindings_expr(expr: &Expr, script_legacy_vars: &mut ScriptLegacyVars) {
    let expr = unroll_paren_seq(expr);

    let Expr::Object(ref obj_lit) = *expr else {
        return;
    };

    collect_obj_fields(obj_lit, &mut script_legacy_vars.data)
}
