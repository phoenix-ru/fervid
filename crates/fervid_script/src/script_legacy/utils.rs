use swc_core::ecma::{
    ast::{
        Callee, Expr, Function, Module, ModuleDecl, ModuleItem, ObjectLit, Prop, PropName,
        PropOrSpread, ReturnStmt, Stmt,
    },
    atoms::{JsWord, js_word},
};

pub fn find_default_export(module: &Module) -> Option<&ObjectLit> {
    let define_component = JsWord::from("defineComponent");

    module.body.iter().find_map(|module_item| {
        let ModuleItem::ModuleDecl(module_decl) = module_item else {
            return None;
        };

        match module_decl {
            ModuleDecl::ExportDefaultExpr(export_default_expr) => {
                match *export_default_expr.expr {
                    // Plain export
                    // `export default { /* ... */ }`
                    Expr::Object(ref object_lit) => {
                        return Some(object_lit);
                    }

                    // When doing `defineComponent`
                    Expr::Call(ref call_expr) => {
                        let Callee::Expr(ref callee_expr) = call_expr.callee else {
                            return None;
                        };

                        // Only support unwrapping `defineComponent`
                        if let Expr::Ident(ref ident) = **callee_expr {
                            if ident.sym != define_component {
                                return None;
                            }
                        } else {
                            return None;
                        };

                        let Some(first_arg) = call_expr.args.get(0) else {
                            return None;
                        };

                        match *first_arg.expr {
                            Expr::Object(ref object_lit) => Some(object_lit),
                            _ => None,
                        }
                    }

                    _ => None,
                }
            }
            _ => None,
        }
    })
}

pub fn find_function(object: &ObjectLit, name: JsWord) -> Option<&Function> {
    object.props.iter().find_map(|prop| match prop {
        PropOrSpread::Prop(prop) => {
            let Prop::Method(ref method) = **prop else {
                return None;
            };

            match method.key {
                PropName::Ident(ref ident) if ident.sym == name => Some(&*method.function),

                PropName::Str(ref s) if s.value == name => Some(&*method.function),

                _ => None,
            }
        }
        _ => None,
    })
}

pub fn find_setup_function(object: &ObjectLit) -> Option<&Function> {
    find_function(object, JsWord::from("setup"))
}

pub fn find_data_function(object: &ObjectLit) -> Option<&Function> {
    find_function(object, js_word!("data"))
}

pub fn find_return(function: &Function) -> Option<&ReturnStmt> {
    let Some(ref fn_body) = function.body else {
        return None;
    };

    fn_body.stmts.iter().find_map(|stmt| match stmt {
        Stmt::Return(ref return_stmt) => Some(return_stmt),

        _ => None
    })
}

pub fn collect_fn_return_fields(function: &Function, out: &mut Vec<JsWord>) {
    let Some(ref return_stmt) = find_return(function) else {
        return;
    };

    let Some(ref return_arg) = return_stmt.arg else {
        return;
    };

    let Expr::Object(ref return_obj) = **return_arg else {
        return;
    };

    collect_obj_fields(return_obj, out);
}

pub fn collect_obj_fields(object: &ObjectLit, out: &mut Vec<JsWord>) {
    for prop in object.props.iter() {
        let PropOrSpread::Prop(prop) = prop else {
            continue;
        };

        match **prop {
            Prop::Shorthand(ref ident) => {
                out.push(ident.sym.clone());
            }
            Prop::KeyValue(ref key_value) => {
                match key_value.key {
                    PropName::Ident(ref ident) => {
                        out.push(ident.sym.clone())
                    }
                    PropName::Str(ref s) => {
                        // Though I don't see how this can be used in a template
                        out.push(s.value.clone())
                    }
                    _ => {}
                }
            },
            Prop::Assign(_) => todo!(),
            Prop::Getter(_) => todo!(),
            Prop::Setter(_) => todo!(),
            Prop::Method(_) => todo!(),
        }
    }
}
