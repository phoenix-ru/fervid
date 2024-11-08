use fervid_core::{BindingTypes, FervidAtom};
use fxhash::FxHashSet;
use itertools::{Either, Itertools};
use swc_core::{common::{Spanned, DUMMY_SP}, ecma::ast::{ArrayLit, CallExpr, Expr, ExprOrSpread, Ident, Lit, Str, TsFnOrConstructorType, TsFnParam, TsLit, TsType}};

use crate::{atoms::EMIT_HELPER, error::{ScriptError, ScriptErrorKind, TransformError}, script::resolve_type::{resolve_type_elements, resolve_union_type, ResolvedElements, TypeResolveContext}, SetupBinding, SfcExportedObjectHelper, TypeOrDecl};

use super::macros::TransformMacroResult;

pub fn process_define_emits(
    ctx: &mut TypeResolveContext,
    call_expr: &CallExpr,
    is_var_decl: bool,
    is_ident: bool,
    var_bindings: Option<&mut Vec<SetupBinding>>,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    _errors: &mut Vec<TransformError>,
) -> TransformMacroResult {
    // Validation: duplicate call
    if sfc_object_helper.emits.is_some() {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: call_expr.span,
            kind: ScriptErrorKind::DuplicateDefineEmits,
        }));
    }

    // Validation: both runtime and types
    if !call_expr.args.is_empty() && call_expr.type_args.is_some() {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: call_expr.span,
            kind: ScriptErrorKind::DefineEmitsTypeAndNonTypeArguments,
        }));
    }

    if let Some(arg0) = &call_expr.args.get(0) {
        sfc_object_helper.emits = Some(arg0.expr.to_owned())
    } else if let Some(ref type_args) = call_expr.type_args {
        let Some(ts_type) = type_args.params.first() else {
            return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
                span: type_args.span,
                kind: ScriptErrorKind::DefineEmitsMalformed,
            }));
        };

        let runtime_emits = match extract_runtime_emits(ctx, &ts_type) {
            Ok(v) => v,
            Err(e) => return TransformMacroResult::Error(TransformError::ScriptError(e)),
        };

        sfc_object_helper.emits = Some(Box::new(Expr::Array(ArrayLit {
            span: DUMMY_SP,
            elems: runtime_emits
                .into_iter()
                .map(|it| {
                    Some(ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Lit(Lit::Str(Str {
                            span: DUMMY_SP,
                            value: it,
                            raw: None,
                        }))),
                    })
                })
                .collect_vec(),
        })))
    }

    // Return `__emits` when in var mode
    // Change binding to be `SetupConst`
    if is_var_decl {
        sfc_object_helper.is_setup_emit_referenced = true;
        
        if let Some(var_bindings) = var_bindings {
            if is_ident && var_bindings.len() == 1 {
                let first_binding = &mut var_bindings[0];
                first_binding.1 = BindingTypes::SetupConst;
            }
        }

        TransformMacroResult::ValidMacro(Some(Box::new(Expr::Ident(Ident {
            span: call_expr.span,
            ctxt: Default::default(),
            sym: EMIT_HELPER.to_owned(),
            optional: false,
        }))))
    } else {
        TransformMacroResult::ValidMacro(None)
    }
}

/// Extracts runtime emits from type-only `defineEmits` declaration
/// Adapted from https://github.com/vuejs/core/blob/0ac0f2e338f6f8f0bea7237db539c68bfafb88ae/packages/compiler-sfc/src/script/defineEmits.ts#L73-L103
fn extract_runtime_emits(
    ctx: &mut TypeResolveContext,
    type_arg: &TsType,
) -> Result<FxHashSet<FervidAtom>, ScriptError> {
    let mut emits = FxHashSet::<FervidAtom>::default();

    // Handle cases like `defineEmits<(e: 'foo' | 'bar') => void>()`
    if let TsType::TsFnOrConstructorType(TsFnOrConstructorType::TsFnType(ref ts_fn_type)) = type_arg
    {
        // Expect first param in fn, e.g. `e: 'foo' | 'bar'` in example above
        let Some(first_fn_param) = ts_fn_type.params.first() else {
            return Err(ScriptError {
                span: ts_fn_type.span,
                kind: ScriptErrorKind::DefineEmitsMalformed,
            });
        };

        extract_event_names(ctx, first_fn_param, &mut emits);

        return Ok(emits);
    }

    let ResolvedElements { props, calls } = resolve_type_elements(ctx, type_arg)?;

    let mut has_property = false;
    for key in props.into_keys() {
        emits.insert(key);
        has_property = true;
    }

    if !calls.is_empty() {
        if has_property {
            return Err(ScriptError {
                kind: ScriptErrorKind::DefineEmitsMixedCallAndPropertySyntax,
                span: type_arg.span(),
            });
        }

        for call in calls {
            let (params, span) = match call {
                Either::Left(l) => (l.params, l.span),
                Either::Right(r) => (r.params, r.span),
            };

            let Some(first_param) = params.first() else {
                return Err(ScriptError {
                    span,
                    kind: ScriptErrorKind::ResolveTypeMissingTypeParam,
                });
            };
            extract_event_names(ctx, first_param, &mut emits);
        }
    }

    return Ok(emits);
}

/// Adapted from https://github.com/vuejs/core/blob/0ac0f2e338f6f8f0bea7237db539c68bfafb88ae/packages/compiler-sfc/src/script/defineEmits.ts#L105-L128
fn extract_event_names(
    ctx: &mut TypeResolveContext,
    event_name: &TsFnParam,
    emits: &mut FxHashSet<FervidAtom>,
) {
    let TsFnParam::Ident(ident) = event_name else {
        return;
    };

    let Some(ref type_annotation) = ident.type_ann else {
        return;
    };

    let scope = ctx.root_scope();
    let scope = scope.borrow();

    let types = resolve_union_type(ctx, &type_annotation.type_ann, &scope);
    for ts_type in types {
        let TypeOrDecl::Type(ts_type) = ts_type else {
            continue;
        };

        if let TsType::TsLitType(ts_lit_type) = ts_type.as_ref() {
            // No UnaryExpression
            match ts_lit_type.lit {
                TsLit::Number(ref n) => {
                    emits.insert(FervidAtom::from(n.value.to_string()));
                }
                TsLit::Str(ref s) => {
                    emits.insert(s.value.to_owned());
                }
                TsLit::Bool(ref b) => {
                    emits.insert(FervidAtom::from(b.value.to_string()));
                }
                TsLit::BigInt(ref big_int) => {
                    emits.insert(FervidAtom::from(big_int.value.to_string()));
                }
                TsLit::Tpl(_) => {}
            }
        }
    }
}
