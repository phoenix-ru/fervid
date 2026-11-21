use fervid_core::BindingTypes;
use swc_core::ecma::ast::{Callee, ClassDecl, Expr, FnDecl, ObjectPatProp, Pat, RestPat};

use crate::{SetupBinding, script::utils::unroll_paren_seq, structs::VueImportAliases};

use super::utils::is_static;

/// Javascript class declaration is always constant.
/// ```js
/// class Foo {}
/// ```
/// will be categorized as `SetupBinding("Foo", BindingTypes::SetupConst)`
#[inline]
pub fn categorize_class(class: &ClassDecl) -> SetupBinding {
    SetupBinding::new_spanned(
        class.ident.sym.to_owned(),
        BindingTypes::SetupConst,
        class.ident.span,
    )
}

/// Javascript function declaration is always constant.
/// ```js
/// function foo() {}
/// ```
/// will be categorized as `SetupBinding("Foo", BindingTypes::SetupConst)`
#[inline]
pub fn categorize_fn_decl(fn_decl: &FnDecl) -> SetupBinding {
    SetupBinding::new_spanned(
        fn_decl.ident.sym.to_owned(),
        BindingTypes::SetupConst,
        fn_decl.ident.span,
    )
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
pub fn categorize_expr(expr: &Expr, vue_user_imports: &VueImportAliases) -> BindingTypes {
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

                    if callee_ident_option == vue_user_imports.ref_import
                        || callee_ident_option == vue_user_imports.computed
                    {
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

        Expr::Await(await_expr) => categorize_expr(&await_expr.arg, vue_user_imports),

        // MaybeRef binding
        Expr::Ident(_) | Expr::Cond(_) | Expr::Member(_) | Expr::OptChain(_) | Expr::Assign(_) => {
            BindingTypes::SetupMaybeRef
        }

        //
        // TS expressions
        //
        Expr::TsTypeAssertion(type_assertion_expr) => {
            categorize_expr(&type_assertion_expr.expr, vue_user_imports)
        }

        Expr::TsConstAssertion(const_assertion_expr) => {
            categorize_expr(&const_assertion_expr.expr, vue_user_imports)
        }

        Expr::TsNonNull(non_null_expr) => categorize_expr(&non_null_expr.expr, vue_user_imports),

        Expr::TsInstantiation(instantiation_expr) => {
            categorize_expr(&instantiation_expr.expr, vue_user_imports)
        }

        Expr::TsAs(as_expr) => categorize_expr(&as_expr.expr, vue_user_imports),

        Expr::TsSatisfies(satisfies_expr) => {
            categorize_expr(&satisfies_expr.expr, vue_user_imports)
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
    collected_bindings: &mut [SetupBinding],
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
        binding.binding_type = rhs_type;
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
        Pat::Ident(decl_ident) => {
            let binding_type = if is_const {
                BindingTypes::SetupMaybeRef
            } else {
                BindingTypes::SetupLet
            };

            out.push(SetupBinding::new_spanned(
                decl_ident.sym.to_owned(),
                binding_type,
                decl_ident.span,
            ));
        }

        // The rest of the function collects the destructures,
        // e.g. `foo` in `const { foo = 123 } = {}` or `bar` in `let [bar] = [123]`

        // `[foo, bar]` in `const [foo, bar] = []
        Pat::Array(arr_destr) => {
            for pat in arr_destr.elems.iter().flatten() {
                extract_variables_from_pat(pat, out, is_const);
            }
        }

        // `...bar` in `const [foo, ...bar] = []` or in `const { foo, ..bar } = {}`
        Pat::Rest(rest_destr) => collect_rest_pat(rest_destr, out, is_const),

        // `foo` in `const { foo } = {}`
        Pat::Object(obj_destr) => {
            for obj_destr_prop in obj_destr.props.iter() {
                match obj_destr_prop {
                    // `foo: bar` in `const { foo: bar } = {}`
                    ObjectPatProp::KeyValue(key_val_destr) => {
                        extract_variables_from_pat(&key_val_destr.value, out, is_const);
                    }

                    // `foo` in `const { foo } = {}` and in `const { foo = 'bar' } = {}`
                    ObjectPatProp::Assign(assign_destr) => out.push(SetupBinding::new_spanned(
                        assign_destr.key.sym.to_owned(),
                        if is_const {
                            BindingTypes::SetupMaybeRef
                        } else {
                            BindingTypes::SetupLet
                        },
                        assign_destr.key.span,
                    )),

                    // `bar` in `const { foo, ...bar } = {}`
                    ObjectPatProp::Rest(rest_destr) => collect_rest_pat(rest_destr, out, is_const),
                }
            }
        }

        Pat::Assign(assign_destr) => {
            extract_variables_from_pat(&assign_destr.left, out, is_const);
        }

        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

/// Collects variables from a `...rest` pattern, including cases like `[...arr]`, `{ ...obj }`.
/// Current Vue js compiler has a bug here for script collection, it returns `undefined` for `const [...[bar]] = smth`
fn collect_rest_pat(rest_pat: &RestPat, out: &mut Vec<SetupBinding>, is_const: bool) {
    if let Pat::Ident(ident) = rest_pat.arg.as_ref() {
        // Binding type is always can be either `SetupConst` or `SetupLet` because
        // rest on ident creates a new array/object
        let binding_type = if is_const {
            BindingTypes::SetupConst
        } else {
            BindingTypes::SetupLet
        };

        out.push(SetupBinding::new_spanned(
            ident.sym.to_owned(),
            binding_type,
            ident.span,
        ))
    } else {
        extract_variables_from_pat(&rest_pat.arg, out, is_const);
    }
}

#[cfg(test)]
mod tests {
    use super::extract_variables_from_pat;
    use crate::test_utils::pat;
    use fervid_core::BindingTypes;

    #[test]
    fn direct_rest_bindings_distinguish_const_and_mutable_declarations() {
        for pattern in ["[...rest]", "{ ...rest }"] {
            assert_bindings(pattern, true, &[("rest", BindingTypes::SetupConst)]);
            assert_bindings(pattern, false, &[("rest", BindingTypes::SetupLet)]);
        }
    }

    // Fervid improvement: collect nested rest bindings instead of porting vuejs-core `undefined` bug (see
    // [collect_rest_pat] for context)
    // https://github.com/vuejs/core/blob/8b479a89445868d474f17e2d837765e7c9ece5e4/packages/compiler-sfc/src/compileScript.ts#L1240-L1311
    #[test]
    fn nested_rest_elements_remain_maybe_refs_while_rest_containers_are_constant() {
        // const [head, ...[value, ...tail]] = source
        assert_bindings(
            "[head, ...[value, ...tail]]",
            true,
            &[
                ("head", BindingTypes::SetupMaybeRef),
                ("value", BindingTypes::SetupMaybeRef),
                ("tail", BindingTypes::SetupConst),
            ],
        );
        // let/var [head, ...[value, ...tail]] = source
        assert_bindings(
            "[head, ...[value, ...tail]]",
            false,
            &[
                ("head", BindingTypes::SetupLet),
                ("value", BindingTypes::SetupLet),
                ("tail", BindingTypes::SetupLet),
            ],
        );
    }

    #[test]
    fn nested_rest_object_patterns_collect_bindings_not_expression_references() {
        // const [...{ length, [key]: value = fallback }] = source
        assert_bindings(
            "[...{ length, [key]: value = fallback }]",
            true,
            &[
                ("length", BindingTypes::SetupMaybeRef),
                ("value", BindingTypes::SetupMaybeRef),
            ],
        );
    }

    fn assert_bindings(pattern: &str, is_const: bool, expected: &[(&str, BindingTypes)]) {
        let mut bindings = Vec::new();
        extract_variables_from_pat(&pat(pattern), &mut bindings, is_const);
        let actual: Vec<_> = bindings
            .iter()
            .map(|binding| (binding.sym.as_str(), binding.binding_type))
            .collect();
        assert_eq!(actual, expected, "pattern={pattern}, is_const={is_const}");
    }
}
