use fervid_core::{BindingTypes, SetupBinding};
use swc_core::ecma::ast::{Callee, ClassDecl, Expr, FnDecl, ObjectPatProp, Pat, RestPat};

use crate::{script::utils::unroll_paren_seq, structs::VueResolvedImports};

use super::utils::is_static;

/// Javascript class declaration is always constant.
/// ```js
/// class Foo {}
/// ```
/// will be categorized as `SetupBinding("Foo", BindingTypes::SetupConst)`
#[inline]
pub fn categorize_class(class: &ClassDecl) -> SetupBinding {
    SetupBinding(class.ident.sym.to_owned(), BindingTypes::SetupConst)
}

/// Javascript function declaration is always constant.
/// ```js
/// function foo() {}
/// ```
/// will be categorized as `SetupBinding("Foo", BindingTypes::SetupConst)`
#[inline]
pub fn categorize_fn_decl(fn_decl: &FnDecl) -> SetupBinding {
    SetupBinding(fn_decl.ident.sym.to_owned(), BindingTypes::SetupConst)
}

/// Categorizes the binding type of an expression (typically RHS).
/// This is specifically made separate from `<script setup>` macros,
/// in order to also work in Options API context.
///
/// Categorization strongly depends on the previously analyzed `vue_user_imports`.
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
pub fn categorize_expr(
    expr: &Expr,
    vue_user_imports: &VueResolvedImports,
    is_await_used: &mut bool,
) -> BindingTypes {
    // Unroll an expression from all possible parenthesis and commas,
    // e.g. `(foo, bar)` -> `bar`
    let expr = unroll_paren_seq(expr);

    match expr {
        // We only support Vue's function calls.
        // If this is not a Vue function, it is either SetupMaybeRef or SetupLet
        Expr::Call(call_expr) => {
            match call_expr.callee {
                Callee::Expr(ref callee_expr) if callee_expr.is_ident() => {
                    let Expr::Ident(ref callee_ident) = **callee_expr else {
                        unreachable!()
                    };

                    // Check Vue atoms (they must have been imported before)
                    // Use PartialEq on Option<Id> for convenience
                    let callee_ident_option = Some(callee_ident.to_id());

                    if callee_ident_option == vue_user_imports.ref_import {
                        BindingTypes::SetupRef
                    } else if callee_ident_option == vue_user_imports.computed {
                        BindingTypes::SetupRef
                    } else if callee_ident_option == vue_user_imports.reactive {
                        BindingTypes::SetupReactiveConst
                    } else {
                        BindingTypes::SetupMaybeRef
                    }
                }

                // This is something unsupported, just add a MaybeRef binding
                _ => BindingTypes::SetupMaybeRef,
            }
        }

        Expr::Await(await_expr) => {
            *is_await_used = true; // only first-level await is recognized
            categorize_expr(&await_expr.arg, vue_user_imports, is_await_used)
        }

        // MaybeRef binding
        Expr::Ident(_) | Expr::Cond(_) | Expr::Member(_) | Expr::OptChain(_) | Expr::Assign(_) => {
            BindingTypes::SetupMaybeRef
        }

        //
        // TS expressions
        //
        Expr::TsTypeAssertion(type_assertion_expr) => {
            categorize_expr(&type_assertion_expr.expr, vue_user_imports, is_await_used)
        }

        Expr::TsConstAssertion(const_assertion_expr) => {
            categorize_expr(&const_assertion_expr.expr, vue_user_imports, is_await_used)
        }

        Expr::TsNonNull(non_null_expr) => {
            categorize_expr(&non_null_expr.expr, vue_user_imports, is_await_used)
        }

        Expr::TsInstantiation(instantiation_expr) => {
            categorize_expr(&instantiation_expr.expr, vue_user_imports, is_await_used)
        }

        Expr::TsAs(as_expr) => categorize_expr(&as_expr.expr, vue_user_imports, is_await_used),

        Expr::TsSatisfies(satisfies_expr) => {
            categorize_expr(&satisfies_expr.expr, vue_user_imports, is_await_used)
        }

        // The other variants are never refs
        // TODO Write tests and check difficult cases, there would be exceptions
        _ if is_static(expr) => BindingTypes::LiteralConst,
        _ => BindingTypes::SetupConst,
    }
}

/// Enriches binding types with additional information obtained from analyzing RHS
#[inline]
pub fn enrich_binding_types(
    collected_bindings: &mut Vec<SetupBinding>,
    rhs_type: BindingTypes,
    is_const: bool,
    is_ident: bool,
) {
    // Skip work when it is not an identifier or a constant (these are good already).
    // This check is needed to ensure consumers don't accidentally call a function
    // and overwrite good values.
    if !is_const || !is_ident {
        return;
    }

    for binding in collected_bindings.iter_mut() {
        binding.1 = rhs_type;
    }
}

/// Extracts the variables from the declarator, e.g.
/// - `foo = 'bar'` -> `foo`;
/// - `{ baz, qux }` -> `baz`, `qux`.
///
/// This also suggests possible binding types `SetupMaybeRef` and `SetupLet`.
/// The suggestions are not final, as this function does not know about the RHS of a variable declaration.
pub fn extract_variables_from_pat(pat: &Pat, out: &mut Vec<SetupBinding>, is_const: bool) {
    match pat {
        // Base case for recursion
        // Idents are easy to collect
        Pat::Ident(ref decl_ident) => {
            let binding_type = if is_const {
                BindingTypes::SetupMaybeRef
            } else {
                BindingTypes::SetupLet
            };

            out.push(SetupBinding(decl_ident.sym.to_owned(), binding_type));
        }

        // The rest of the function collects the destructures,
        // e.g. `foo` in `const { foo = 123 } = {}` or `bar` in `let [bar] = [123]`

        // `[foo, bar]` in `const [foo, bar] = []
        Pat::Array(arr_destr) => {
            for arr_destr_elem in arr_destr.elems.iter() {
                if let Some(pat) = arr_destr_elem {
                    extract_variables_from_pat(pat, out, is_const);
                }
            }
        }

        // `...bar` in `const [foo, ...bar] = []` or in `const { foo, ..bar } = {}`
        Pat::Rest(rest_destr) => collect_rest_pat(rest_destr, out),

        // `foo` in `const { foo } = {}`
        Pat::Object(obj_destr) => {
            for obj_destr_prop in obj_destr.props.iter() {
                match obj_destr_prop {
                    // `foo: bar` in `const { foo: bar } = {}`
                    ObjectPatProp::KeyValue(key_val_destr) => {
                        extract_variables_from_pat(&key_val_destr.value, out, is_const);
                    }

                    // `foo` in `const { foo } = {}` and in `const { foo = 'bar' } = {}`
                    ObjectPatProp::Assign(assign_destr) => out.push(SetupBinding(
                        assign_destr.key.sym.to_owned(),
                        if is_const {
                            BindingTypes::SetupMaybeRef
                        } else {
                            BindingTypes::SetupLet
                        },
                    )),

                    // `bar` in `const { foo, ...bar } = {}`
                    ObjectPatProp::Rest(rest_destr) => collect_rest_pat(rest_destr, out),
                }
            }
        }

        Pat::Assign(assign_destr) => {
            extract_variables_from_pat(&assign_destr.left, out, is_const);
        }

        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

#[inline]
fn collect_rest_pat(rest_pat: &RestPat, out: &mut Vec<SetupBinding>) {
    // Only `...ident` is supported.
    // Current Vue js compiler has a bug, it returns `undefined` for `...[bar]`
    if let Some(ident) = rest_pat.arg.as_ident() {
        // Binding type is always `SetupConst` because of the nature of rest operator
        out.push(SetupBinding(ident.sym.to_owned(), BindingTypes::SetupConst))
    };
}
