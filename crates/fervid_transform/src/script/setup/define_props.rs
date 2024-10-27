use fervid_core::{fervid_atom, BindingTypes, FervidAtom, IntoIdent, VueImports};
use flagset::FlagSet;
use itertools::Itertools;
use swc_core::{
    common::{Span, Spanned, DUMMY_SP},
    ecma::ast::{
        ArrayLit, Bool, CallExpr, Callee, Expr, ExprOrSpread, GetterProp, IdentName, KeyValueProp,
        Lit, MethodProp, ObjectLit, Prop, PropName, PropOrSpread, SetterProp, TsType,
    },
};

use crate::{
    atoms::{DEFINE_PROPS, PROPS_HELPER},
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::{
        resolve_type::{
            infer_runtime_type_resolved_prop, resolve_type_elements, ResolutionResult,
            ResolvedPropValue, TypeResolveContext, Types, TypesSet,
        },
        utils::{collect_obj_fields, collect_string_arr},
    },
    SetupBinding, SfcExportedObjectHelper,
};

use super::macros::TransformMacroResult;

#[derive(Default)]
struct DefineProps {
    span: Span,
    runtime_decl: Option<Box<Expr>>,
    type_decl: Option<Box<TsType>>,
    // /// What to replace the macro for (e.g. `const props = defineProps()` -> `const props = __props`)
    // variable: Option<Box<Expr>>,
    /// Second argument of `withDefaults`
    defaults: Option<Box<Expr>>,
}

pub fn process_define_props(
    ctx: &mut TypeResolveContext,
    call_expr: &CallExpr,
    is_var_decl: bool,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> TransformMacroResult {
    let mut define_props = DefineProps::default();
    extract_from_define_props(call_expr, &mut define_props);
    process_define_props_impl(ctx, define_props, is_var_decl, sfc_object_helper)
}

pub fn process_with_defaults(
    ctx: &mut TypeResolveContext,
    with_defaults_call: &CallExpr,
    is_var_decl: bool,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> TransformMacroResult {
    macro_rules! bail_no_define_props {
        () => {
            return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
                span: with_defaults_call.span,
                kind: ScriptErrorKind::WithDefaultsWithoutDefineProps,
            }))
        };
    }

    // 1. Validate that first argument is `defineProps`
    let Some(first_arg) = with_defaults_call.args.first() else {
        bail_no_define_props!()
    };

    let Expr::Call(define_props_call) = first_arg.expr.as_ref() else {
        bail_no_define_props!()
    };

    let Callee::Expr(ref define_props_callee) = define_props_call.callee else {
        bail_no_define_props!()
    };

    if !matches!(define_props_callee.as_ref(), Expr::Ident(i) if DEFINE_PROPS.eq(&i.sym)) {
        bail_no_define_props!()
    };

    // Extract from `defineProps`
    let mut define_props = DefineProps::default();
    extract_from_define_props(define_props_call, &mut define_props);

    // Extract from `withDefaults`
    define_props.defaults = with_defaults_call.args.get(1).map(|v| v.expr.to_owned());

    // Process
    process_define_props_impl(ctx, define_props, is_var_decl, sfc_object_helper)

    // TODO Implement a more generic `process_define_props_impl` function
    // which will return values to be assembled by `process_define_props` and `process_with_defaults`.
    // The values needed:
    // - props Expr;
    // - raw bindings;
    // - type or not type.
}

/// Extracts runtime and types from `defineProps` call
fn extract_from_define_props(define_props_call: &CallExpr, out: &mut DefineProps) {
    // Runtime
    if let Some(first_argument) = &define_props_call.args.get(0) {
        out.runtime_decl = Some(first_argument.expr.to_owned());
    }

    // Types
    if let Some(ref type_args) = define_props_call.type_args {
        out.type_decl = type_args.params.first().map(|v| v.to_owned());
    }
}

fn process_define_props_impl(
    ctx: &mut TypeResolveContext,
    define_props: DefineProps,
    is_var_decl: bool,
    sfc_object_helper: &mut SfcExportedObjectHelper,
) -> TransformMacroResult {
    // Check duplicate
    if sfc_object_helper.props.is_some() {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: define_props.span,
            kind: ScriptErrorKind::DuplicateDefineProps,
        }));
    }

    // Check runtime and types co-usage
    if let (Some(_), Some(types)) = (&define_props.runtime_decl, &define_props.type_decl) {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: types.span(),
            kind: ScriptErrorKind::DefinePropsTypeAndNonTypeArguments,
        }));
    }

    // Check runtime and `withDefaults` co-usage
    if define_props.runtime_decl.is_some() && define_props.defaults.is_some() {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span: define_props.span,
            kind: ScriptErrorKind::WithDefaultsNeedsTypeOnlyDefineProps,
        }));
    }

    // Calculate result
    let props_expr = if let Some(runtime_decl) = define_props.runtime_decl {
        // Add props as bindings
        let mut raw_bindings = Vec::new();
        match runtime_decl.as_ref() {
            Expr::Array(props_arr) => {
                collect_string_arr(props_arr, &mut raw_bindings);
            }
            Expr::Object(props_obj) => {
                collect_obj_fields(props_obj, &mut raw_bindings);
            }
            _ => {}
        }

        ctx.bindings_helper.setup_bindings.extend(
            raw_bindings
                .into_iter()
                .map(|raw| SetupBinding(raw, BindingTypes::Props)),
        );

        Some(runtime_decl)
    } else if let Some(type_decl) = define_props.type_decl {
        let extracted_props_result =
            extract_runtime_props(ctx, &type_decl, define_props.defaults.as_deref());

        match extracted_props_result {
            Ok(v) => v,
            Err(e) => return TransformMacroResult::Error(TransformError::ScriptError(e)),
        }
    } else {
        // When both runtime and types are absent (i.e. for `defineProps()`), we allow it
        None
    };

    // Assign result.
    // LHS is guaranteed to be None because of the duplicate check.
    sfc_object_helper.props = props_expr;

    // Return `__props` when in var mode. None otherwise - still a valid macro
    if is_var_decl {
        sfc_object_helper.is_setup_props_referenced = true;

        TransformMacroResult::ValidMacro(Some(Box::new(Expr::Ident(
            PROPS_HELPER
                .to_owned()
                .into_ident_spanned(define_props.span),
        ))))
    } else {
        TransformMacroResult::ValidMacro(None)
    }
}

// Adapted from https://github.com/vuejs/core/blob/3bda3e83fd9e2fbe451a1c79dae82ff6a7467683/packages/compiler-sfc/src/script/defineProps.ts

struct PropTypeData {
    key: FervidAtom,
    types: TypesSet,
    required: bool,
    skip_check: bool,
}

/// Convert type-only props declaration to a runtime value
fn extract_runtime_props(
    ctx: &mut TypeResolveContext,
    type_decl: &TsType,
    defaults: Option<&Expr>,
) -> ResolutionResult<Option<Box<Expr>>> {
    let props = resolve_runtime_props_from_type(ctx, type_decl)?;
    if props.is_empty() {
        return Ok(None);
    }

    let has_static_defaults = has_static_with_defaults(defaults);
    let mut props_obj = ObjectLit {
        span: DUMMY_SP,
        props: Vec::with_capacity(props.len()),
    };

    for prop in props {
        let key = prop.key.clone();

        props_obj.props.push(get_runtime_prop_from_type(
            ctx,
            prop,
            defaults,
            has_static_defaults,
        ));

        // Register binding if not registered already
        // TODO Need to check the case with
        // ```
        // defineProps<{
        //   foo: number
        // }>()
        // const foo = ref() // <- this should prevail
        // ```
        if !ctx
            .bindings_helper
            .setup_bindings
            .iter()
            .any(|it| it.0 == key)
        {
            ctx.bindings_helper
                .setup_bindings
                .push(SetupBinding(key, BindingTypes::Props));
        }
    }

    let mut props_decl = Box::new(Expr::Object(props_obj));

    // Has defaults, but they are not static
    if let (false, Some(defaults)) = (has_static_defaults, defaults) {
        let merge_defaults_helper = VueImports::MergeDefaults;
        ctx.bindings_helper.vue_imports |= merge_defaults_helper;

        // TODO /*#__PURE__*/ comment
        props_decl = Box::new(Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Ident(
                merge_defaults_helper.as_atom().into_ident(),
            ))),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: props_decl,
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(defaults.to_owned()),
                },
            ],
            type_args: None,
        }));
    }

    Ok(Some(props_decl))
}

fn resolve_runtime_props_from_type(
    ctx: &mut TypeResolveContext,
    type_decl: &TsType,
) -> ResolutionResult<Vec<PropTypeData>> {
    let mut props = vec![];
    let elements = resolve_type_elements(ctx, type_decl)?;

    for (key, element) in elements.props {
        let mut types = infer_runtime_type_resolved_prop(ctx, &element);

        // Skip check for result containing unknown types
        let mut skip_check = false;
        if types.contains(Types::Unknown) {
            if types.contains(Types::Boolean | Types::Function) {
                types -= Types::Unknown;
                skip_check = true;
            } else {
                types = FlagSet::from(Types::Null);
            }
        }

        let required = match element.value {
            ResolvedPropValue::TsPropertySignature(s) => !s.optional,
            ResolvedPropValue::TsMethodSignature(s) => !s.optional,
        };

        props.push(PropTypeData {
            key,
            types,
            required,
            skip_check,
        });
    }

    return Ok(props);
}

fn get_runtime_prop_from_type(
    ctx: &mut TypeResolveContext,
    prop: PropTypeData,
    defaults: Option<&Expr>,
    has_static_defaults: bool,
) -> PropOrSpread {
    let mut default: Option<Box<Prop>> = None;
    let default_prop_name = PropName::Ident(IdentName {
        span: DUMMY_SP,
        sym: fervid_atom!("default"),
    });

    let PropTypeData { key, .. } = prop;

    let destructured = gen_destructured_default_value(ctx, &key);
    if let Some(destructured) = destructured {
        default = Some(Box::new(Prop::KeyValue(KeyValueProp {
            key: default_prop_name,
            value: destructured.value,
        })));
    } else if has_static_defaults {
        let Some(Expr::Object(defaults)) = defaults else {
            unreachable!("has_static_defaults can only be true when defaults are present")
        };

        for iterated_prop in defaults.props.iter() {
            let PropOrSpread::Prop(iterated_prop) = iterated_prop else {
                continue;
            };

            /// Checks match of iterated prop against prop we are looking for
            macro_rules! key_matches {
                ($source: ident) => {
                    match $source.key {
                        PropName::Ident(ref ident) => &key == &ident.sym,
                        PropName::Str(ref s) => &key == &s.value,
                        PropName::Num(ref n) => &key == &n.value.to_string(),
                        PropName::Computed(_) => false,
                        PropName::BigInt(_) => false,
                    }
                };
            }

            match iterated_prop.as_ref() {
                // Equivalent of `ObjectProperty`
                Prop::KeyValue(key_value) => {
                    if key_matches!(key_value) {
                        default = Some(Box::new(Prop::KeyValue(KeyValueProp {
                            key: default_prop_name,
                            value: key_value.value.to_owned(),
                        })));
                        break;
                    }
                }
                Prop::Shorthand(shorthand) => {
                    if &key == &shorthand.sym {
                        default = Some(Box::new(Prop::KeyValue(KeyValueProp {
                            key: default_prop_name,
                            value: Box::new(Expr::Ident(shorthand.to_owned())),
                        })));
                        break;
                    }
                }

                // Equivalent of `ObjectMethod`
                Prop::Getter(getter) => {
                    if key_matches!(getter) {
                        default = Some(Box::new(Prop::Getter(GetterProp {
                            span: getter.span,
                            key: default_prop_name,
                            type_ann: getter.type_ann.to_owned(),
                            body: getter.body.to_owned(),
                        })));
                        break;
                    }
                }
                Prop::Setter(setter) => {
                    if key_matches!(setter) {
                        default = Some(Box::new(Prop::Setter(SetterProp {
                            span: setter.span,
                            key: default_prop_name,
                            this_param: setter.this_param.to_owned(),
                            param: setter.param.to_owned(),
                            body: setter.body.to_owned(),
                        })));
                        break;
                    }
                }
                Prop::Method(method) => {
                    if key_matches!(method) {
                        default = Some(Box::new(Prop::Method(MethodProp {
                            key: default_prop_name,
                            function: method.function.to_owned(),
                        })));
                        break;
                    }
                }

                // Not applicable to `ObjectLit` (SWC)
                Prop::Assign(_) => {}
            }
        }
    }

    // Difference from official compiler: no key escaping happens, as we rely on SWC

    // For return value
    let mut prop_object_fields: Vec<PropOrSpread> = Vec::with_capacity(4);

    macro_rules! add_field {
        ($name: literal, $value: expr) => {
            prop_object_fields.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: fervid_atom!($name),
                }),
                value: $value,
            }))))
        };
    }

    macro_rules! return_value {
        ($prop_object_fields: ident) => {
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                // TODO Better span is probably possible (preserve prop name span)
                key: PropName::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: key.to_owned(),
                }),
                value: Box::new(Expr::Object(ObjectLit {
                    // TODO Better span is probably possible (preserve prop defaults span?)
                    span: DUMMY_SP,
                    props: $prop_object_fields,
                })),
            })))
        };
    }

    if !ctx.bindings_helper.is_prod {
        // e.g. `type: Number`
        add_field!("type", to_runtime_type_string(prop.types));

        // e.g. `required: true`
        add_field!(
            "required",
            Box::new(Expr::Lit(Lit::Bool(Bool {
                // TODO We may know the span if we try to preserve it
                span: DUMMY_SP,
                value: prop.required,
            })))
        );

        // `skipCheck: true`
        if prop.skip_check {
            add_field!(
                "skipCheck",
                Box::new(Expr::Lit(Lit::Bool(Bool {
                    // TODO We may know the span if we try to preserve it
                    span: DUMMY_SP,
                    value: true,
                })))
            )
        }

        // e.g. `default: 0`
        if let Some(default) = default {
            prop_object_fields.push(PropOrSpread::Prop(default));
        }

        return return_value!(prop_object_fields);
    }

    // #8989 for custom element, should keep the type
    // #4783 for boolean, should keep the type
    // #7111 for function, if default value exists or it's not static, should keep it
    // in production
    let default_defined_or_not_static = !has_static_defaults || default.is_some();
    let types = prop.types;
    if ctx.is_ce
        || types.contains(Types::Boolean)
        || (default_defined_or_not_static && types.contains(Types::Function))
    {
        // e.g. `type: Number`
        add_field!("type", to_runtime_type_string(types));

        // e.g. `default: 0`
        if let Some(default) = default {
            prop_object_fields.push(PropOrSpread::Prop(default));
        }

        return return_value!(prop_object_fields);
    }

    // Production: checks are useless
    let prop_object_fields = if let Some(default) = default {
        vec![PropOrSpread::Prop(default)]
    } else {
        vec![]
    };

    return return_value!(prop_object_fields);
}

/// Check defaults. If the default object is an object literal with only
/// static properties, we can directly generate more optimized default
/// declarations. Otherwise we will have to fallback to runtime merging.
fn has_static_with_defaults(defaults: Option<&Expr>) -> bool {
    let Some(Expr::Object(obj)) = defaults else {
        return false;
    };

    obj.props.iter().all(|prop_or_spread| {
        let PropOrSpread::Prop(prop) = prop_or_spread else {
            return false;
        };

        let key = match prop.as_ref() {
            Prop::KeyValue(kv) => &kv.key,
            Prop::Getter(getter) => &getter.key,
            Prop::Setter(setter) => &setter.key,
            Prop::Method(method) => &method.key,
            // No key here, assume to be static
            Prop::Shorthand(_) => return false,
            // This is not in the ObjectLit
            Prop::Assign(_) => return true,
        };

        !matches!(key, PropName::Computed(_))
    })
}

struct GenDestructuredDefaultValueReturn {
    value: Box<Expr>,
    _need_skip_factory: bool,
}
fn gen_destructured_default_value(
    _ctx: &mut TypeResolveContext,
    _key: &str,
) -> Option<GenDestructuredDefaultValueReturn> {
    // TODO
    None
}

fn to_runtime_type_string(types: TypesSet) -> Box<Expr> {
    let mut idents: Vec<&'static str> = Vec::new();

    for type_name in types.into_iter() {
        idents.push(type_name.into())
    }

    if idents.len() == 1 {
        return Box::new(Expr::Ident(FervidAtom::from(idents[0]).into_ident()));
    }

    let array_elems = idents
        .into_iter()
        .map(|ident| {
            Some(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(FervidAtom::from(ident).into_ident())),
            })
        })
        .collect_vec();

    Box::new(Expr::Array(ArrayLit {
        span: DUMMY_SP,
        elems: array_elems,
    }))
}
