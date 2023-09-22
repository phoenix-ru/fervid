use fervid_core::BindingTypes;
use swc_core::ecma::ast::{
    Decl, Stmt, VarDecl,
    VarDeclKind,
};

use crate::{
    script::common::{categorize_class, categorize_fn_decl, categorize_var_declarator},
    structs::{SetupBinding, SfcExportedObjectHelper, VueResolvedImports},
};

use super::macros::transform_script_setup_macro_expr_stmt;

/// Analyzes the statement in `script setup` context.
/// This can either be:
/// 1. The whole body of `<script setup>`;
/// 2. The top-level statements in `<script>` when using mixed `<script>` + `<script setup>`;
/// 3. The insides of `setup()` function in `<script>` Options API.
pub fn transform_and_record_stmt(
    stmt: &Stmt,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> Option<Stmt> {
    match stmt {
        Stmt::Expr(ref expr) => {
            return transform_script_setup_macro_expr_stmt(expr, sfc_object_helper).map(Stmt::Expr)
        }

        Stmt::Decl(decl) => {
            transform_decl_stmt(decl, out, vue_imports, sfc_object_helper);
        }

        _ => {}
    }

    // By default, just return the copied statement
    Some(stmt.to_owned())
}

/// Analyzes the declaration in `script setup` context.
/// These are typically `var`/`let`/`const` declarations, function declarations, etc.
fn transform_decl_stmt(
    decl: &Decl,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports,
    sfc_object_helper: &mut SfcExportedObjectHelper
) {
    match decl {
        Decl::Class(class) => out.push(categorize_class(class)),

        Decl::Fn(fn_decl) => out.push(categorize_fn_decl(fn_decl)),

        Decl::Var(var_decl) => transform_and_record_var_decls(var_decl, out, vue_imports),

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
}

// TODO The algorithms are a bit different for <script>, <script setup> and setup()
// I still need to test how it works MORE

/// Categorizes `var`/`let`/`const` declaration block
/// which may include multiple variables.
/// Categorization strongly depends on the previously analyzed `vue_imports`.
///
/// ## Examples
/// ```js
/// import { ref, computed, reactive } from 'vue'
///
/// let foo = ref(1)                    // BindingTypes::SetupLet
/// const
///     pi = 3.14,                      // BindingTypes::LiteralConst
///     bar = ref(2),                   // BindingTypes::SetupRef
///     baz = computed(() => 3),        // BindingTypes::SetupRef
///     qux = reactive({ x: 4 }),       // BindingTypes::SetupReactiveConst
/// ```
///
pub fn transform_and_record_var_decls(
    var_decl: &VarDecl,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports,
) {
    let is_const = matches!(var_decl.kind, VarDeclKind::Const);

    for decl in var_decl.decls.iter() {
        categorize_var_declarator(&decl, out, vue_imports, is_const);
        // TODO Process macros
    }
}
