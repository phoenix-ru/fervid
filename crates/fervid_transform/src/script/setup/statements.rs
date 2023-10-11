use fervid_core::BindingTypes;
use swc_core::ecma::ast::{Decl, ExprStmt, Stmt, VarDeclKind};

use crate::{
    script::common::{categorize_class, categorize_fn_decl, categorize_var_declarator},
    structs::{SetupBinding, SfcExportedObjectHelper, VueResolvedImports},
};

use super::macros::transform_script_setup_macro_expr;

/// Analyzes the statement in `script setup` context.
/// This can either be:
/// 1. The whole body of `<script setup>`;
/// 2. The top-level statements in `<script>` when using mixed `<script>` + `<script setup>`;
/// 3. The insides of `setup()` function in `<script>` Options API.
pub fn transform_and_record_stmt(
    stmt: Stmt,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Stmt> {
    match stmt {
        Stmt::Expr(ref expr_stmt) => {
            let span = expr_stmt.span;
            return transform_script_setup_macro_expr(&expr_stmt.expr, sfc_object_helper, false)
                .map(|transformed| {
                    Stmt::Expr(ExprStmt {
                        span,
                        expr: Box::new(transformed),
                    })
                });
        }

        Stmt::Decl(ref decl) => {
            return transform_decl_stmt(decl, out, vue_imports, sfc_object_helper).map(Stmt::Decl);
        }

        _ => {}
    }

    // By default, just return the copied statement
    Some(stmt)
}

/// Analyzes the declaration in `script setup` context.
/// These are typically `var`/`let`/`const` declarations, function declarations, etc.
fn transform_decl_stmt(
    decl: &Decl,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Decl> {
    match decl {
        Decl::Class(class) => out.push(categorize_class(class)),

        Decl::Fn(fn_decl) => out.push(categorize_fn_decl(fn_decl)),

        Decl::Var(var_decl) => {
            let is_const = matches!(var_decl.kind, VarDeclKind::Const);

            // We need to clone the whole declaration to mutate its decls.
            // I am not too happy by the amount of copying done everywhere. Maybe it should mutate instead?..
            let mut new_var_decl = var_decl.to_owned();

            for var_declarator in new_var_decl.as_mut().decls.iter_mut() {
                categorize_var_declarator(&var_declarator, out, vue_imports, is_const);

                if let Some(ref mut init_expr) = var_declarator.init {
                    let transformed = transform_script_setup_macro_expr(&init_expr, sfc_object_helper, true);
                    if let Some(transformed) = transformed {
                        *init_expr = Box::new(transformed);
                    }
                }
            }

            return Some(Decl::Var(new_var_decl));
        }

        Decl::TsEnum(ts_enum) => {
            // Ambient enums are also included, this is intentional
            // I am not sure about `const enum`s though
            out.push(SetupBinding(
                ts_enum.id.sym.to_owned(),
                BindingTypes::LiteralConst,
            ))
        }

        // TODO: What?
        // Decl::TsInterface(_) => todo!(),
        // Decl::TsTypeAlias(_) => todo!(),
        // Decl::TsModule(_) => todo!(),
        _ => {}
    }

    // By default, just return the copied declaration
    Some(decl.to_owned())
}
