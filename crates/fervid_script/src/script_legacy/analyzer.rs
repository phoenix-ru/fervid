use swc_core::ecma::{
    ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, Callee, Decl, Expr, Function, Lit, Module,
        ModuleDecl, ModuleItem, ObjectLit, Pat, Prop, PropName, PropOrSpread, Stmt, VarDeclKind, Tpl,
    },
    atoms::JsWord,
};

use super::{
    components::collect_components_object,
    computed::collect_computed_object,
    data::{collect_data_bindings_block_stmt, collect_data_bindings_expr},
    directives::collect_directives_object,
    emits::{collect_emits_bindings_array, collect_emits_bindings_object},
    expose::collect_expose_bindings_array,
    imports::collect_imports,
    inject::{collect_inject_bindings_array, collect_inject_bindings_object},
    methods::collect_methods_object,
    props::{collect_prop_bindings_array, collect_prop_bindings_object},
    setup::{collect_setup_bindings_block_stmt, collect_setup_bindings_expr},
    utils::{unroll_paren_seq, get_string_tpl},
    ScriptLegacyVars,
};
use crate::{
    atoms::*,
    structs::{BindingTypes, SetupBinding, VueResolvedImports},
};

pub fn analyze_default_export(default_export: &ObjectLit, out: &mut ScriptLegacyVars) {
    // tl;dr Visit every method, arrow function, object or array and forward control
    for field in default_export.props.iter() {
        let PropOrSpread::Prop(prop) = field else {
            continue;
        };

        match **prop {
            Prop::KeyValue(ref key_value) => {
                let sym = match key_value.key {
                    PropName::Ident(ref ident) => &ident.sym,
                    PropName::Str(ref s) => &s.value,
                    _ => continue,
                };

                match *key_value.value {
                    Expr::Array(ref array_lit) => handle_options_array(sym, array_lit, out),
                    Expr::Object(ref obj_lit) => handle_options_obj(sym, obj_lit, out),
                    Expr::Fn(ref fn_expr) => handle_options_function(sym, &fn_expr.function, out),
                    Expr::Arrow(ref arrow_expr) => {
                        handle_options_arrow_function(sym, arrow_expr, out)
                    }
                    Expr::Lit(ref lit) => handle_options_lit(sym, lit, out),
                    Expr::Tpl(ref tpl) => handle_options_tpl(sym, tpl, out),

                    // These latter types technically can be analyzed as well,
                    // because they only need `.expr` unwrapping and re-matching.
                    // It can be done when the match moves into a function
                    // which can be recursively called.
                    // Expr::TsTypeAssertion(_) => todo!(),
                    // Expr::TsConstAssertion(_) => todo!(),
                    // Expr::TsAs(_) => todo!(),
                    _ => {
                        continue;
                    }
                }
            }
            Prop::Method(ref method) => {
                let sym = match method.key {
                    PropName::Ident(ref ident) => &ident.sym,
                    PropName::Str(ref s) => &s.value,
                    _ => continue,
                };

                handle_options_function(sym, &method.function, out)
            }
            _ => {}
        }
    }
}

pub fn analyze_top_level_items(
    module: &Module,
    out: &mut ScriptLegacyVars,
    vue_imports: &mut VueResolvedImports,
) {
    for module_item in module.body.iter() {
        match *module_item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(ref import_decl)) => {
                collect_imports(import_decl, out, vue_imports)
            }

            ModuleItem::Stmt(ref stmt) => analyze_top_level_stmt(stmt, out, vue_imports),

            // Exports are ignored
            _ => {}
        }
    }
}

#[inline]
fn analyze_top_level_stmt(
    stmt: &Stmt,
    out: &mut ScriptLegacyVars,
    vue_imports: &mut VueResolvedImports,
) {
    match stmt {
        Stmt::Decl(decl) => match decl {
            Decl::Class(class) => out.setup.push(SetupBinding(
                class.ident.sym.to_owned(),
                BindingTypes::SetupConst,
            )),

            Decl::Fn(fn_decl) => out.setup.push(SetupBinding(
                fn_decl.ident.sym.to_owned(),
                BindingTypes::SetupConst,
            )),

            Decl::Var(var_decl) => {
                let is_const = matches!(var_decl.kind, VarDeclKind::Const);

                for decl in var_decl.decls.iter() {
                    // SOOQA JAVASCRIPT BLYAT
                    match decl.name {
                        Pat::Ident(ref decl_ident) => {
                            macro_rules! push {
                                ($typ: expr) => {
                                    out.setup
                                        .push(SetupBinding(decl_ident.sym.to_owned(), $typ))
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

            // TODO: What?
            Decl::TsInterface(_) => todo!(),
            Decl::TsTypeAlias(_) => todo!(),
            Decl::TsEnum(_) => todo!(),
            Decl::TsModule(_) => todo!(),
        },

        _ => {}
    }
}

/// In Options API, `props`, `inject`, `emits` and `expose` may be arrays
fn handle_options_array(
    field: &JsWord,
    array_lit: &ArrayLit,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    if *field == *PROPS {
        collect_prop_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *INJECT {
        collect_inject_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *EMITS {
        collect_emits_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *EXPOSE {
        collect_expose_bindings_array(array_lit, script_legacy_vars)
    }
}

/// Similar to [handle_options_array], only `data`, `setup` may be declared as arrow fns
fn handle_options_arrow_function(
    field: &JsWord,
    arrow_expr: &ArrowExpr,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    // Arrow functions may either have a body or an expression
    // `() => {}` is a body which returns nothing
    // `() => ({})` is an expression which returns an empty object
    macro_rules! forward_block_stmt_or_expr {
        ($forward_block_stmt: ident, $forward_expr: ident) => {
            match *arrow_expr.body {
                BlockStmtOrExpr::BlockStmt(ref block_stmt) => {
                    $forward_block_stmt(block_stmt, script_legacy_vars)
                }
                BlockStmtOrExpr::Expr(ref arrow_body_expr) => {
                    $forward_expr(arrow_body_expr, script_legacy_vars)
                }
            }
        };
    }

    // It reads a bit cryptic because of the macro calls,
    // but you should only care about the functions which are called,
    // e.g. [`collect_data_bindings_block_stmt`]
    if *field == *DATA {
        forward_block_stmt_or_expr!(collect_data_bindings_block_stmt, collect_data_bindings_expr);
    } else if *field == *SETUP {
        forward_block_stmt_or_expr!(
            collect_setup_bindings_block_stmt,
            collect_setup_bindings_expr
        )
    }
}

/// Same as [handle_options_arrow_function], `data` and `setup`
fn handle_options_function(
    field: &JsWord,
    function: &Function,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    let Some(ref function_body) = function.body else {
        return;
    };

    if *field == *DATA {
        collect_data_bindings_block_stmt(function_body, script_legacy_vars)
    } else if *field == *SETUP {
        collect_setup_bindings_block_stmt(function_body, script_legacy_vars)
    }
}

/// `name`
fn handle_options_lit(field: &JsWord, lit: &Lit, script_legacy_vars: &mut ScriptLegacyVars) {
    if *field == *NAME {
        if let Lit::Str(name) = lit {
            script_legacy_vars.name = Some(name.value.to_owned())
        }
    }
}

/// `name`
fn handle_options_tpl(field: &JsWord, tpl: &Tpl, script_legacy_vars: &mut ScriptLegacyVars) {
    if *field == *NAME {
        script_legacy_vars.name = get_string_tpl(tpl);
    }
}

/// `props`, `computed`, `inject`, `emits`, `components`, `methods`, `directives`
fn handle_options_obj(
    field: &JsWord,
    obj_lit: &ObjectLit,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    if *field == *PROPS {
        collect_prop_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *COMPUTED {
        collect_computed_object(obj_lit, script_legacy_vars)
    } else if *field == *INJECT {
        collect_inject_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *EMITS {
        collect_emits_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *COMPONENTS {
        collect_components_object(obj_lit, script_legacy_vars)
    } else if *field == *METHODS {
        collect_methods_object(obj_lit, script_legacy_vars)
    } else if *field == *DIRECTIVES {
        collect_directives_object(obj_lit, script_legacy_vars)
    }
}
