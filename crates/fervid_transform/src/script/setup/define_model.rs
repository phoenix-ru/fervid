use fervid_core::{fervid_atom, BindingTypes, VueImports};
use swc_core::ecma::ast::{
    Bool, CallExpr, Callee, Expr, ExprOrSpread, Ident, IdentName, KeyValueProp, Lit, ObjectLit,
    Prop, PropName, PropOrSpread, Str,
};

use crate::{
    atoms::{MODEL_VALUE, PROPS_HELPER, USE_MODEL_HELPER},
    BindingsHelper, SetupBinding, SfcDefineModel, SfcExportedObjectHelper,
};

use super::macros::TransformMacroResult;

pub fn process_define_model(
    call_expr: &CallExpr,
    is_var_decl: bool,
    is_ident: bool,
    var_bindings: Option<&mut Vec<SetupBinding>>,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    bindings_helper: &mut BindingsHelper,
) -> TransformMacroResult {
    let define_model = read_define_model(&call_expr.args);
    let span = call_expr.span;

    // Add to imports
    bindings_helper.vue_imports |= VueImports::UseModel;

    let use_model_ident = Ident {
        span,
        ctxt: Default::default(),
        sym: USE_MODEL_HELPER.to_owned(),
        optional: false,
    };

    let mut use_model_args =
        Vec::<ExprOrSpread>::with_capacity(if define_model.local { 3 } else { 2 });

    // __props
    sfc_object_helper.is_setup_props_referenced = true;
    use_model_args.push(ExprOrSpread {
        spread: None,
        expr: Box::new(Expr::Ident(Ident {
            span,
            ctxt: Default::default(),
            sym: PROPS_HELPER.to_owned(),
            optional: false,
        })),
    });

    // "model-name"
    let model_name = define_model.name.to_owned();
    use_model_args.push(ExprOrSpread {
        spread: None,
        expr: Box::new(Expr::Lit(Lit::Str(Str {
            span,
            value: model_name.to_owned(),
            raw: None,
        }))),
    });

    // `{ local: true }` if needed
    if define_model.local {
        use_model_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Object(ObjectLit {
                span,
                props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(IdentName {
                        span,
                        sym: fervid_atom!("local"),
                    }),
                    value: Box::new(Expr::Lit(Lit::Bool(Bool { span, value: true }))),
                })))],
            })),
        })
    }

    sfc_object_helper.models.push(define_model);

    // Binding type of the prop
    bindings_helper
        .setup_bindings
        .push(SetupBinding(model_name, BindingTypes::Props));

    // Binding type of the model itself
    if let (true, true, Some(var_bindings)) = (is_var_decl, is_ident, var_bindings) {
        if var_bindings.len() == 1 {
            let binding = &mut var_bindings[0];
            binding.1 = BindingTypes::SetupRef;
        }
    }

    // _useModel(__props, "model-name", %model options%)
    TransformMacroResult::ValidMacro(Some(Box::new(Expr::Call(CallExpr {
        span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(use_model_ident))),
        args: use_model_args,
        type_args: None,
    }))))
}

/// Processes `defineModel`
fn read_define_model(macro_args: &[ExprOrSpread]) -> SfcDefineModel {
    // 1st arg - model name (string) or model options (object)
    let first_arg = macro_args.get(0);

    // 2nd arg - model options (object)
    let second_arg = macro_args.get(1);

    // Get name. It may be a first argument, or may be omitted altogether (defaults to `modelValue`)
    let (name, is_first_arg_name) = match first_arg {
        Some(ExprOrSpread { spread: None, expr }) => match **expr {
            Expr::Lit(Lit::Str(ref name)) => (name.value.to_owned(), true),
            _ => (MODEL_VALUE.to_owned(), false),
        },

        _ => (MODEL_VALUE.to_owned(), false),
    };

    let options: Option<&ExprOrSpread> = if is_first_arg_name {
        second_arg
    } else {
        first_arg
    };

    // Check if options is an object, we'll need `local` option from it
    let local = is_local(options);

    SfcDefineModel {
        name,
        local,
        options: options.map(|o| Box::new(o.to_owned())),
    }
}

/// Dig into options and find `local` field in the object with a boolean value.
/// If property is not found or `options` is not a proper object, `false` is returned.
fn is_local(options: Option<&ExprOrSpread>) -> bool {
    let Some(ExprOrSpread { spread: None, expr }) = options else {
        return false;
    };

    let Expr::Object(ref obj) = **expr else {
        return false;
    };

    let local_prop_value = obj.props.iter().find_map(|prop| match prop {
        PropOrSpread::Prop(prop) => {
            let Prop::KeyValue(ref key_value) = **prop else {
                return None;
            };

            match key_value.key {
                PropName::Ident(ref ident) if ident.sym == fervid_atom!("local") => {
                    Some(&key_value.value)
                }

                PropName::Str(ref s) if s.value == fervid_atom!("local") => Some(&key_value.value),

                _ => None,
            }
        }
        _ => None,
    });

    let Some(local_prop_value) = local_prop_value else {
        return false;
    };

    let Expr::Lit(Lit::Bool(ref local_bool)) = **local_prop_value else {
        return false;
    };

    local_bool.value
}
