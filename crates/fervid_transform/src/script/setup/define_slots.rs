use fervid_core::VueImports;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{CallExpr, Callee, Expr, Ident},
};

use crate::{
    error::{ScriptError, ScriptErrorKind, TransformError},
    BindingsHelper, SfcExportedObjectHelper,
};

use super::macros::TransformMacroResult;

pub fn process_define_slots(
    call_expr: &CallExpr,
    is_var_decl: bool,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    bindings_helper: &mut BindingsHelper,
) -> TransformMacroResult {
    if sfc_object_helper.has_define_slots {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: call_expr.span,
            kind: ScriptErrorKind::DuplicateDefineSlots,
        }));
    }
    sfc_object_helper.has_define_slots = true;

    if !call_expr.args.is_empty() {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: call_expr.span,
            kind: ScriptErrorKind::DefineSlotsArguments,
        }));
    }

    // `defineSlots` without a variable declaration
    if !is_var_decl {
        return TransformMacroResult::ValidMacro(None);
    }

    // Add to imports and get the identifier
    bindings_helper.vue_imports |= VueImports::UseSlots;
    let use_slots_ident = Ident {
        span: DUMMY_SP,
        ctxt: Default::default(),
        sym: VueImports::UseSlots.as_atom(),
        optional: false,
    };

    // _useSlots()
    TransformMacroResult::ValidMacro(Some(Box::new(Expr::Call(CallExpr {
        span: call_expr.span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(use_slots_ident))),
        args: vec![],
        type_args: None,
    }))))
}
