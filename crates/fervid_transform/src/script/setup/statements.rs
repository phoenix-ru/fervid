use fervid_core::{BindingTypes, BindingsHelper, SetupBinding};
use swc_core::ecma::ast::{Decl, ExprStmt, Stmt, VarDeclKind};

use crate::{
    script::common::{categorize_class, categorize_fn_decl, categorize_var_declarator},
    structs::{SfcExportedObjectHelper, VueResolvedImports},
};

use super::macros::transform_script_setup_macro_expr;

/// Analyzes the statement in `script setup` context.
/// This can either be:
/// 1. The whole body of `<script setup>`;
/// 2. The top-level statements in `<script>` when using mixed `<script>` + `<script setup>`;
/// 3. The insides of `setup()` function in `<script>` Options API.
pub fn transform_and_record_stmt(
    stmt: Stmt,
    bindings_helper: &mut BindingsHelper,
    vue_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Stmt> {
    match stmt {
        Stmt::Expr(expr_stmt) => {
            let span = expr_stmt.span;
            transform_script_setup_macro_expr(
                *expr_stmt.expr,
                bindings_helper,
                sfc_object_helper,
                false,
            )
            .map(|transformed| {
                Stmt::Expr(ExprStmt {
                    span,
                    expr: Box::new(transformed),
                })
            })
        }

        Stmt::Decl(decl) => {
            transform_decl_stmt(decl, bindings_helper, vue_imports, sfc_object_helper)
                .map(Stmt::Decl)
        }

        // By default, just return the same statement
        _ => Some(stmt),
    }
}

/// Analyzes the declaration in `script setup` context.
/// These are typically `var`/`let`/`const` declarations, function declarations, etc.
fn transform_decl_stmt(
    decl: Decl,
    bindings_helper: &mut BindingsHelper,
    vue_user_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Decl> {
    /// Pushes the binding type and returns the same passed `Decl`
    macro_rules! push_return {
        ($binding: expr) => {
            bindings_helper.setup_bindings.push($binding);
            // By default, just return the same declaration
            return Some(decl);
        };
    }

    match decl {
        Decl::Class(ref class) => {
            push_return!(categorize_class(class));
        }

        Decl::Fn(ref fn_decl) => {
            push_return!(categorize_fn_decl(fn_decl));
        }

        Decl::Var(mut var_decl) => {
            let is_const = matches!(var_decl.kind, VarDeclKind::Const);

            for var_declarator in var_decl.as_mut().decls.iter_mut() {
                categorize_var_declarator(
                    &var_declarator,
                    &mut bindings_helper.setup_bindings,
                    vue_user_imports,
                    is_const,
                );

                // TODO Mutate instead of `take`ing?

                if let Some(init_expr) = var_declarator.init.take() {
                    let transformed = transform_script_setup_macro_expr(
                        *init_expr,
                        bindings_helper,
                        sfc_object_helper,
                        true,
                    );
                    var_declarator.init = transformed.map(Box::new);
                }
            }

            Some(Decl::Var(var_decl))
        }

        Decl::TsEnum(ref ts_enum) => {
            // Ambient enums are also included, this is intentional
            // I am not sure about `const enum`s though
            push_return!(SetupBinding(
                ts_enum.id.sym.to_owned(),
                BindingTypes::LiteralConst,
            ));
        }

        // TODO: What?
        // Decl::TsInterface(_) => todo!(),
        // Decl::TsTypeAlias(_) => todo!(),
        // Decl::TsModule(_) => todo!(),
        _ => Some(decl),
    }
}
