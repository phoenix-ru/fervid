use swc_core::{
    common::{Spanned, DUMMY_SP},
    ecma::ast::{CallExpr, Expr, ExprOrSpread, Prop, PropOrSpread},
};

use crate::{
    error::{ScriptError, ScriptErrorKind, TransformError}, script::setup::utils::unwrap_ts_node_expr, SfcExportedObjectHelper
};

use super::macros::TransformMacroResult;

pub fn process_define_options(
    call_expr: &CallExpr,
    is_var_decl: bool,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    errors: &mut Vec<TransformError>,
) -> TransformMacroResult {
    // A variable is not a correct usage
    if is_var_decl {
        return TransformMacroResult::NotAMacro;
    }

    macro_rules! valid_macro {
        ($return_value: expr) => {
            TransformMacroResult::ValidMacro($return_value)
        };
    }

    macro_rules! error {
        ($error_value: expr) => {
            TransformMacroResult::Error($error_value)
        };
    }

    if sfc_object_helper.has_define_options {
        return error!(TransformError::ScriptError(ScriptError {
            span: call_expr.span,
            kind: ScriptErrorKind::DuplicateDefineOptions
        }));
    }

    if let Some(ref type_args) = call_expr.type_args.as_ref() {
        return error!(TransformError::ScriptError(ScriptError {
            span: type_args.span,
            kind: ScriptErrorKind::DefineOptionsTypeArguments
        }));
    }

    // `defineOptions()` without arguments
    let Some(ExprOrSpread { spread: None, expr }) = call_expr.args.get(0) else {
        return valid_macro!(None);
    };

    // Mark `defineOptions` as present to prevent duplicates
    sfc_object_helper.has_define_options = true;

    // Unwrap from TS
    let expr = unwrap_ts_node_expr(expr);

    // Try to take out object, otherwise just use spread
    let Expr::Object(ref options_object) = *expr else {
        sfc_object_helper.untyped_fields.push(PropOrSpread::Spread(
            swc_core::ecma::ast::SpreadElement {
                dot3_token: DUMMY_SP,
                expr: Box::new(expr.to_owned()),
            },
        ));
        return valid_macro!(None);
    };

    // Error when `props`, `emits`, `expose` or `slots` are inside the object
    let mut error = None;
    for prop in options_object.props.iter() {
        let PropOrSpread::Prop(prop) = prop else {
            continue;
        };
        let key = match prop.as_ref() {
            Prop::Shorthand(ident) => Some(&ident.sym),
            Prop::KeyValue(key_value_prop) => key_value_prop.key.as_ident().map(|v| &v.sym),
            Prop::Assign(_) => None,
            Prop::Getter(getter_prop) => getter_prop.key.as_ident().map(|v| &v.sym),
            Prop::Setter(setter_prop) => setter_prop.key.as_ident().map(|v| &v.sym),
            Prop::Method(method_prop) => method_prop.key.as_ident().map(|v| &v.sym),
        };
        let Some(key) = key else {
            continue;
        };

        // Assign the first encountered error as "hard" error and others as "soft" errors.
        // This is better than in the official compiler which only does 1 error at a time.
        macro_rules! add_error {
            ($kind: ident) => {{
                let new_error = TransformError::ScriptError(ScriptError {
                    span: prop.span(),
                    kind: ScriptErrorKind::$kind,
                });
                if error.is_some() {
                    errors.push(new_error);
                } else {
                    error = Some(new_error);
                }
            }};
        }
        match key.as_str() {
            "props" => add_error!(DefineOptionsProps),
            "emits" => add_error!(DefineOptionsEmits),
            "expose" => add_error!(DefineOptionsExpose),
            "slots" => add_error!(DefineOptionsSlots),
            _ => {}
        }
    }
    if let Some(error) = error {
        return error!(error);
    }

    // Copy the fields
    sfc_object_helper
        .untyped_fields
        .extend(options_object.props.iter().cloned());

    valid_macro!(None)
}
