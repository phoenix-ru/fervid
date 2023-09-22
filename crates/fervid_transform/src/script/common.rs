use fervid_core::BindingTypes;
use swc_core::ecma::ast::{ClassDecl, FnDecl, VarDeclarator, Pat, Expr, Callee, ObjectPatProp, RestPat};

use crate::{structs::{SetupBinding, VueResolvedImports}, script::utils::unroll_paren_seq};

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

/// Collects the identifiers from `foo = 42` and `bar = 'baz'` separately in `const foo = 42, bar = 'baz'`
pub fn categorize_var_declarator(
    var_decl: &VarDeclarator,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports,
    is_const: bool,
) {
    // Handle destructures separately. Rest of this function does not care about them.
    let Pat::Ident(ref decl_ident) = var_decl.name else {
        collect_destructure(&var_decl.name, out, is_const);
        return;
    };

    macro_rules! push {
        ($typ: expr) => {
            out.push(SetupBinding(decl_ident.sym.to_owned(), $typ))
        };
    }

    // For `let` and `var` type is always BindingTypes::SetupLet
    if !is_const {
        push!(BindingTypes::SetupLet);
        return;
    }

    // If no init expr, that means this is not a const anyways
    // `let tmp;` is valid, but `const tmp;` is not
    let Some(ref init_expr) = var_decl.init else {
        push!(BindingTypes::SetupLet);
        return;
    };

    let init_expr = unroll_paren_seq(init_expr);

    match init_expr {
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

                    if callee_ident_option == vue_imports.ref_import {
                        push!(BindingTypes::SetupRef)
                    } else if callee_ident_option == vue_imports.computed {
                        push!(BindingTypes::SetupRef)
                    } else if callee_ident_option == vue_imports.reactive {
                        push!(BindingTypes::SetupReactiveConst)
                    } else {
                        push!(BindingTypes::SetupMaybeRef)
                    }
                }

                // This is something unsupported, just add a MaybeRef binding
                _ => {
                    push!(BindingTypes::SetupMaybeRef);
                    return;
                }
            }
        },

        // MaybeRef binding
        // Expr::Await(_) => todo!(),
        Expr::Ident(_) => {
            push!(BindingTypes::SetupMaybeRef);
        }

        // The other variants are never refs
        _ => {
            push!(BindingTypes::SetupConst)
        }

        // Idk what to do with these
        // Expr::TsTypeAssertion(_) => todo!(),
        // Expr::TsConstAssertion(_) => todo!(),
        // Expr::TsNonNull(_) => todo!(),
        // Expr::TsAs(_) => todo!(),
        // Expr::TsInstantiation(_) => todo!(),
        // Expr::TsSatisfies(_) => todo!(),
    }
}

/// Collects the destructures, e.g. `foo` in `const { foo = 123 } = {}` or `bar` in `let [bar] = [123]`
fn collect_destructure(dest: &Pat, out: &mut Vec<SetupBinding>, is_const: bool) {
    match dest {
        // Base case for recursion
        Pat::Ident(ident) => out.push(SetupBinding(
            ident.sym.to_owned(),
            if is_const {
                BindingTypes::SetupMaybeRef
            } else {
                BindingTypes::SetupLet
            },
        )),

        // `[foo, bar]` in `const [foo, bar] = []
        Pat::Array(arr_destr) => {
            for arr_destr_elem in arr_destr.elems.iter() {
                let Some(arr_destr_elem) = arr_destr_elem else {
                    continue;
                };

                collect_destructure(arr_destr_elem, out, is_const)
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
                        collect_destructure(&key_val_destr.value, out, is_const)
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

        Pat::Assign(assign_destr) => collect_destructure(&assign_destr.left, out, is_const),

        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

#[inline]
fn collect_rest_pat(rest_pat: &RestPat, out: &mut Vec<SetupBinding>) {
    // Only `...ident` is supported.
    // Current Vue js compiler has a bug, it returns `undefined` for `...[bar]`
    let Some(ident) = rest_pat.arg.as_ident() else {
        return;
    };

    // Binding type is always `SetupConst` because of the nature of rest operator
    out.push(SetupBinding(ident.sym.to_owned(), BindingTypes::SetupConst))
}
