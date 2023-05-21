use swc_core::ecma::ast::{BlockStmt, Expr};

use super::{ScriptLegacyVars, utils::{collect_block_stmt_return_fields, unroll_paren_seq, collect_obj_fields}};

/// Collects all the bindings from `setup`, e.g. `setup() { return { foo: 'bar', baz: 42 } }`
///
/// https://vuejs.org/api/composition-api-setup.html
#[inline]
pub fn collect_setup_bindings_block_stmt(block_stmt: &BlockStmt, script_legacy_vars: &mut ScriptLegacyVars) {
    collect_block_stmt_return_fields(block_stmt, &mut script_legacy_vars.setup)
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

    collect_obj_fields(obj_lit, &mut script_legacy_vars.setup)
}
