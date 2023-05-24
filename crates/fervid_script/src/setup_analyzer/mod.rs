use swc_core::ecma::ast::{ClassDecl, FnDecl, VarDecl, VarDeclKind, Pat, Expr, Callee, Stmt, Decl};

use crate::{structs::{SetupBinding, BindingTypes, VueResolvedImports}, common::utils::unroll_paren_seq};

/// Analyzes the statement in `script setup` context.
/// This can either be:
/// 1. The whole body of `<script setup>`;
/// 2. The top-level statements in `<script>` when using mixed `<script>` + `<script setup>`;
/// 3. The insides of `setup()` function in `<script>` Options API.
pub fn analyze_stmt(
    stmt: &Stmt,
    out: &mut Vec<SetupBinding>,
    vue_imports: &mut VueResolvedImports,
) {
    match stmt {
        Stmt::Decl(decl) => match decl {
            Decl::Class(class) => out.push(categorize_class(class)),

            Decl::Fn(fn_decl) => out.push(categorize_fn_decl(fn_decl)),

            Decl::Var(var_decl) => categorize_var_decls(var_decl, out, vue_imports),

            Decl::TsEnum(ts_enum) => {
                // Ambient enums are also included, this is intentional
                // I am not sure about `const enum`s though
                out.push(SetupBinding(ts_enum.id.sym.to_owned(), BindingTypes::LiteralConst))
            },

            // TODO: What?
            // Decl::TsInterface(_) => todo!(),
            // Decl::TsTypeAlias(_) => todo!(),
            // Decl::TsModule(_) => todo!(),
            _ => {}
        },

        _ => {}
    }
}


/// Javascript class declaration is always constant.
/// ```js
/// class Foo {}
/// ```
/// will be categorized as `SetupBinding("Foo", BindingTypes::SetupConst)`
#[inline]
pub fn categorize_class(class: &ClassDecl) -> SetupBinding {
    SetupBinding(
        class.ident.sym.to_owned(),
        BindingTypes::SetupConst,
    )
}

/// Javascript function declaration is always constant.
/// ```js
/// function foo() {}
/// ```
/// will be categorized as `SetupBinding("Foo", BindingTypes::SetupConst)`
#[inline]
pub fn categorize_fn_decl(fn_decl: &FnDecl) -> SetupBinding {
    SetupBinding(
        fn_decl.ident.sym.to_owned(),
        BindingTypes::SetupConst,
    )
}

// TODO The algorithms are actually QUITE different for <script>, <script setup> and setup()
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
pub fn categorize_var_decls(
    var_decl: &VarDecl,
    out: &mut Vec<SetupBinding>,
    vue_imports: &VueResolvedImports
) {
    let is_const = matches!(var_decl.kind, VarDeclKind::Const);

    for decl in var_decl.decls.iter() {
        match decl.name {
            Pat::Ident(ref decl_ident) => {
                macro_rules! push {
                    ($typ: expr) => {
                        out.push(SetupBinding(decl_ident.sym.to_owned(), $typ))
                    };
                }

                // For `let` and `var` type is always BindingTypes::SetupLet
                if !is_const {
                    push!(BindingTypes::SetupLet);
                    continue;
                }

                // If no init expr, that means this is not a const anyways
                // `let tmp;` is valid, but `const tmp;` is not
                let Some(ref init_expr) = decl.init else {
                    push!(BindingTypes::SetupLet);
                    continue;
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
                                continue;
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

            // TODO handle destructures
            // I hate js...
            Pat::Array(_) => todo!(),
            Pat::Rest(_) => todo!(),
            Pat::Object(_) => todo!(),
            Pat::Assign(_) => todo!(),
            Pat::Invalid(_) => todo!(),
            _ => {}
        }
    }
}
