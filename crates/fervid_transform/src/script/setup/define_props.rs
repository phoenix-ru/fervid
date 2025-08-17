use fervid_core::{
    atom_to_propname, fervid_atom, str_to_propname, BindingTypes, FervidAtom, IntoIdent, VueImports,
};
use flagset::FlagSet;
use swc_core::{
    common::{Span, Spanned, DUMMY_SP},
    ecma::ast::{
        ArrayLit, ArrowExpr, BindingIdent, BlockStmtOrExpr, Bool, CallExpr, Callee, Expr,
        ExprOrSpread, GetterProp, IdentName, KeyValueProp, Lit, MethodProp, ObjectLit, ParenExpr,
        Pat, Prop, PropName, PropOrSpread, SetterProp, Str, TsType, VarDeclarator,
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
        setup::utils::to_runtime_type_string,
        utils::{collect_obj_fields, collect_string_arr},
    },
    PropsDestructureBinding, PropsDestructureConfig, SetupBinding, SfcExportedObjectHelper,
};

use super::{
    macros::{TransformMacroResult, VarDeclHelper},
    utils::unwrap_ts_node_expr,
};

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
    var_decl: Option<VarDeclHelper>,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    errors: &mut Vec<TransformError>,
) -> TransformMacroResult {
    let mut define_props = DefineProps::default();
    extract_from_define_props(call_expr, &mut define_props);
    process_define_props_impl(ctx, define_props, var_decl, sfc_object_helper, errors)
}

pub fn process_with_defaults(
    ctx: &mut TypeResolveContext,
    with_defaults_call: &CallExpr,
    var_decl: Option<VarDeclHelper>,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    errors: &mut Vec<TransformError>,
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

    // Check co-usage of props destructure together with `withDefaults`
    if let Some(VarDeclHelper {
        lhs: Pat::Object(obj_pat),
        ..
    }) = var_decl
    {
        // TODO This is technically a warning
        errors.push(TransformError::ScriptError(ScriptError {
            span: obj_pat.span,
            kind: ScriptErrorKind::DefinePropsDestructureUnnecessaryWithDefaults,
        }));
    }

    // Extract from `defineProps`
    let mut define_props = DefineProps::default();
    extract_from_define_props(define_props_call, &mut define_props);

    // Extract from `withDefaults`
    define_props.defaults = with_defaults_call.args.get(1).map(|v| v.expr.to_owned());

    // Process
    process_define_props_impl(ctx, define_props, var_decl, sfc_object_helper, errors)

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
    if let Some(first_argument) = &define_props_call.args.first() {
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
    var_decl: Option<VarDeclHelper>,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    errors: &mut Vec<TransformError>,
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

    // Extract some variables while `define_props` is still owned
    let define_props_span = define_props.span;
    let has_defaults = define_props.defaults.is_some();

    // Calculate result
    let props_expr = match gen_runtime_props(ctx, define_props, errors) {
        Ok(v) => v,
        Err(e) => return TransformMacroResult::Error(e),
    };

    // Assign result.
    // LHS is guaranteed to be None because of the duplicate check.
    sfc_object_helper.props = props_expr;

    // TODO: This technically depends on the usage inside `<template>`,
    // however, by the order of operations scripts are processed earlier than `<template>`,
    // so usage can't be properly inferred yet
    // https://github.com/phoenix-ru/fervid/issues/65
    sfc_object_helper.is_setup_props_referenced = true;

    // When not in var mode - remove the `defineProps` statement
    let Some(var_decl) = var_decl else {
        return TransformMacroResult::ValidMacro(None);
    };

    // Binding type of the prop variable itself
    if var_decl.lhs.is_ident() && var_decl.bindings.len() == 1 {
        let binding = &mut var_decl.bindings[0];
        binding.binding_type = BindingTypes::SetupReactiveConst;
    } else if var_decl.is_const {
        let binding_type = if matches!(ctx.props_destructure, PropsDestructureConfig::True) {
            BindingTypes::Props
        } else {
            BindingTypes::SetupConst
        };

        // `defineProps` with a destructured const variable is `SetupConst`
        var_decl
            .bindings
            .iter_mut()
            .for_each(|v| v.binding_type = binding_type);
    }

    // When define props destructure is used, remove the `defineProps` variable declaration completely
    if !has_defaults
        && var_decl.lhs.is_object()
        && !ctx.bindings_helper.props_destructured_bindings.is_empty()
    {
        // Clear the collected bindings to not accidentally overwrite them
        var_decl.bindings.clear();

        // When `...rest` spread parameter is present, rewrite the whole statement with `createPropsRestProxy`
        if let Some(ref destructure_rest_id) = ctx.bindings_helper.props_destructure_rest_id {
            ctx.bindings_helper.vue_imports |= VueImports::CreatePropsRestProxy;

            let mut create_props_rest_proxy_args = Vec::<ExprOrSpread>::with_capacity(2);

            // __props
            create_props_rest_proxy_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(PROPS_HELPER.to_owned().into_ident())),
            });

            // Other (non-spread) destructured props, e.g. `['foo', 'bar']` in `const { foo, bar, ...rest } = defineProps()`
            let mut props_destructured_bindings_arr = Vec::<Option<ExprOrSpread>>::with_capacity(
                ctx.bindings_helper.props_destructured_bindings.len(),
            );
            for (key, _) in ctx.bindings_helper.props_destructured_bindings.iter() {
                props_destructured_bindings_arr.push(Some(ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: key.to_owned(),
                        raw: None,
                    }))),
                }));
            }
            create_props_rest_proxy_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Array(ArrayLit {
                    span: DUMMY_SP,
                    elems: props_destructured_bindings_arr,
                })),
            });

            // e.g. `createPropsRestProxy(__props, ['foo', 'bar'])`
            let create_props_rest_proxy = Expr::Call(CallExpr {
                span: DUMMY_SP,
                ctxt: Default::default(),
                callee: Callee::Expr(Box::new(Expr::Ident(
                    VueImports::CreatePropsRestProxy.as_atom().into_ident(),
                ))),
                args: create_props_rest_proxy_args,
                type_args: None,
            });

            return TransformMacroResult::ValidMacroRewriteDeclarator(Some(Box::new(
                VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent {
                        id: destructure_rest_id.to_owned().into_ident(),
                        type_ann: None,
                    }),
                    init: Some(Box::new(create_props_rest_proxy)),
                    definite: Default::default(),
                },
            )));
        }

        // When no rest spread, simply remove the statement
        return TransformMacroResult::ValidMacroRewriteDeclarator(None);
    }

    // Return `__props` when in var mode
    TransformMacroResult::ValidMacro(Some(Box::new(Expr::Ident(
        PROPS_HELPER
            .to_owned()
            .into_ident_spanned(define_props_span),
    ))))
}

// Adapted from https://github.com/vuejs/core/blob/3bda3e83fd9e2fbe451a1c79dae82ff6a7467683/packages/compiler-sfc/src/script/defineProps.ts

fn gen_runtime_props(
    ctx: &mut TypeResolveContext,
    define_props: DefineProps,
    errors: &mut Vec<TransformError>,
) -> Result<Option<Box<Expr>>, TransformError> {
    if let Some(runtime_decl) = define_props.runtime_decl {
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
                .map(|raw| SetupBinding::new(raw, BindingTypes::Props)),
        );

        if ctx.bindings_helper.props_destructured_bindings.is_empty() {
            // In contrast to official compiler, models are merged in a separate place
            // TODO Maybe reconsider?
            return Ok(Some(runtime_decl));
        }

        let mut defaults = Vec::<PropOrSpread>::with_capacity(
            ctx.bindings_helper.props_destructured_bindings.len() * 2,
        );

        for (key, binding) in ctx.bindings_helper.props_destructured_bindings.iter() {
            let Some(d) = gen_destructured_default_value(binding, FlagSet::default(), errors)
            else {
                continue;
            };

            // Push the default value
            defaults.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: str_to_propname(key, DUMMY_SP),
                value: d.value,
            }))));

            // Push `__skip_PROPNAME: true`
            if d.need_skip_factory {
                let mut new_key = String::with_capacity(/* "__skip_".len() */ 7 + key.len());
                new_key.push_str("__skip_");
                new_key.push_str(key);

                defaults.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: str_to_propname(&new_key, DUMMY_SP),
                    value: Box::new(Expr::Lit(Lit::Bool(Bool {
                        span: DUMMY_SP,
                        value: true,
                    }))),
                }))));
            }
        }

        if defaults.is_empty() {
            return Ok(Some(runtime_decl));
        }

        Ok(Some(wrap_in_merge_defaults(
            ctx,
            runtime_decl,
            Box::new(Expr::Object(ObjectLit {
                props: defaults,
                span: DUMMY_SP,
            })),
        )))
    } else if let Some(type_decl) = define_props.type_decl {
        let extracted_props_result =
            extract_runtime_props(ctx, &type_decl, define_props.defaults.as_deref(), errors);

        extracted_props_result.map_err(TransformError::ScriptError)
    } else {
        // Allow both runtime and types to be absent (i.e. for `defineProps()`)
        Ok(None)
    }
}

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
    errors: &mut Vec<TransformError>,
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

        props_obj.props.push(gen_runtime_prop_from_type(
            ctx,
            prop,
            defaults,
            has_static_defaults,
            errors,
        ));

        // Register binding if not registered already
        if !ctx
            .bindings_helper
            .setup_bindings
            .iter()
            .any(|it| it.sym == key)
        {
            ctx.bindings_helper
                .setup_bindings
                .push(SetupBinding::new(key, BindingTypes::Props));
        }
    }

    let mut props_decl = Box::new(Expr::Object(props_obj));

    // Has defaults, but they are not static
    if let (false, Some(defaults)) = (has_static_defaults, defaults) {
        let merge_defaults_helper = VueImports::MergeDefaults;
        ctx.bindings_helper.vue_imports |= merge_defaults_helper;

        props_decl = wrap_in_merge_defaults(ctx, props_decl, Box::new(defaults.to_owned()));
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
            if types.contains(Types::Boolean) || types.contains(Types::Function) {
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

    Ok(props)
}

fn gen_runtime_prop_from_type(
    ctx: &mut TypeResolveContext,
    prop: PropTypeData,
    defaults: Option<&Expr>,
    has_static_defaults: bool,
    errors: &mut Vec<TransformError>,
) -> PropOrSpread {
    let mut default: Option<Box<Prop>> = None;
    let default_prop_name = PropName::Ident(IdentName {
        span: DUMMY_SP,
        sym: fervid_atom!("default"),
    });

    let PropTypeData { key, .. } = prop;

    let destructured = ctx
        .bindings_helper
        .props_destructured_bindings
        .iter()
        .find_map(|(k, v)| {
            if k != &key {
                return None;
            }

            gen_destructured_default_value(v, prop.types, errors)
        });

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
                        PropName::Computed(ref c) => match c.expr.as_ref() {
                            Expr::Lit(Lit::Str(s)) => &key == &s.value,
                            Expr::Lit(Lit::Num(n)) => &key == &n.value.to_string(),
                            _ => false,
                        },
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
                    if key == shorthand.sym {
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

    // TODO Better span is probably possible (preserve prop name span)
    let key = atom_to_propname(key.to_owned(), DUMMY_SP);

    macro_rules! return_value {
        ($prop_object_fields: ident) => {
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key,
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

    return_value!(prop_object_fields)
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

        match key {
            PropName::Computed(computed_prop_name) => computed_prop_name.expr.is_lit(),
            _ => true,
        }
    })
}

// https://github.com/vuejs/core/blob/4f792535e25af6941d1eb267fe16e4121623a006/packages/compiler-sfc/src/script/defineProps.ts#L326
struct GenDestructuredDefaultValueReturn {
    value: Box<Expr>,
    need_skip_factory: bool,
}
fn gen_destructured_default_value(
    destructured_binding: &PropsDestructureBinding,
    inferred_type: FlagSet<Types>,
    errors: &mut Vec<TransformError>,
) -> Option<GenDestructuredDefaultValueReturn> {
    let default_val = destructured_binding.default.as_ref()?;

    let unwrapped = unwrap_ts_node_expr(default_val);

    // Check previously inferred type against the naively inferred type of the default value
    if !inferred_type.is_empty() && !inferred_type.contains(Types::Null) {
        let value_type = infer_value_type(unwrapped);
        if let Some(value_type) = value_type {
            if !inferred_type.contains(value_type) {
                errors.push(TransformError::ScriptError(ScriptError {
                    span: unwrapped.span(),
                    kind: ScriptErrorKind::DefinePropsDestructureDeclaredTypeMismatch,
                }));
                return None;
            }
        }
    }

    let need_skip_factory = inferred_type.is_empty()
        && matches!(unwrapped, Expr::Fn(_) | Expr::Arrow(_) | Expr::Ident(_));

    let need_factory_wrap = !need_skip_factory
        && !matches!(unwrapped, Expr::Lit(_))
        && !inferred_type.contains(Types::Function);

    let value = if need_factory_wrap {
        // Fix the SWC stringifier which does not automatically wrap `Expr::Object` in `()`
        // when used as a return value of ArrowExpr.
        // This leads to `() => {}` (empty body) after stringification,
        // which is not the same as `() => ({})` (empty object)
        let arrow_val = if default_val.is_object() {
            Box::new(Expr::Paren(ParenExpr {
                expr: default_val.to_owned(),
                span: DUMMY_SP,
            }))
        } else {
            default_val.to_owned()
        };

        Box::new(Expr::Arrow(ArrowExpr {
            body: Box::new(BlockStmtOrExpr::Expr(arrow_val)),
            ..Default::default()
        }))
    } else {
        default_val.to_owned()
    };

    Some(GenDestructuredDefaultValueReturn {
        value,
        need_skip_factory,
    })
}

fn infer_value_type(expr: &Expr) -> Option<Types> {
    match expr {
        Expr::Lit(Lit::Str(_)) => Some(Types::String),
        Expr::Lit(Lit::Num(_)) => Some(Types::Number),
        Expr::Lit(Lit::Bool(_)) => Some(Types::Boolean),
        Expr::Object(_) => Some(Types::Object),
        Expr::Array(_) => Some(Types::Array),
        Expr::Fn(_) | Expr::Arrow(_) => Some(Types::Function),

        _ => None,
    }
}

fn wrap_in_merge_defaults(
    ctx: &mut TypeResolveContext,
    props_decl: Box<Expr>,
    defaults: Box<Expr>,
) -> Box<Expr> {
    let merge_defaults_helper = VueImports::MergeDefaults;
    ctx.bindings_helper.vue_imports |= merge_defaults_helper;

    // TODO /*#__PURE__*/ comment
    Box::new(Expr::Call(CallExpr {
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
                expr: defaults,
            },
        ],
        type_args: None,
    }))
}
