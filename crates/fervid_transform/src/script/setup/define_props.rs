use fervid_core::{fervid_atom, BindingTypes, FervidAtom};
use swc_core::{
    common::{Span, Spanned, DUMMY_SP},
    ecma::ast::{
        Bool, CallExpr, Callee, Expr, GetterProp, Ident, KeyValueProp, Lit, MethodProp, ObjectLit,
        Prop, PropName, PropOrSpread, SetterProp, TsType,
    },
};

use crate::{
    atoms::{DEFINE_PROPS, PROPS_HELPER},
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::{
        resolve_type::TypeResolveContext,
        utils::{collect_obj_fields, collect_string_arr},
    },
    BindingsHelper, SetupBinding, SfcExportedObjectHelper,
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
    bindings_helper: &mut BindingsHelper,
) -> TransformMacroResult {
    let mut define_props = DefineProps::default();
    extract_from_define_props(call_expr, &mut define_props);
    process_define_props_impl(
        ctx,
        define_props,
        is_var_decl,
        sfc_object_helper,
        bindings_helper,
    )
}

pub fn process_with_defaults(
    ctx: &mut TypeResolveContext,
    with_defaults_call: &CallExpr,
    is_var_decl: bool,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    bindings_helper: &mut BindingsHelper,
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
    process_define_props_impl(
        ctx,
        define_props,
        is_var_decl,
        sfc_object_helper,
        bindings_helper,
    )

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
    bindings_helper: &mut BindingsHelper,
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

        bindings_helper.setup_bindings.extend(
            raw_bindings
                .into_iter()
                .map(|raw| SetupBinding(raw, BindingTypes::Props)),
        );

        Some(runtime_decl)
    } else if let Some(type_decl) = define_props.type_decl {
        extract_runtime_props(
            ctx,
            &type_decl,
            define_props.defaults.as_deref(),
            bindings_helper,
        )
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

        TransformMacroResult::ValidMacro(Some(Box::new(Expr::Ident(Ident {
            span: define_props.span,
            sym: PROPS_HELPER.to_owned(),
            optional: false,
        }))))
    } else {
        TransformMacroResult::ValidMacro(None)
    }
}

// Adapted from https://github.com/vuejs/core/blob/3bda3e83fd9e2fbe451a1c79dae82ff6a7467683/packages/compiler-sfc/src/script/defineProps.ts

struct PropTypeData {
    key: FervidAtom,
    types: Vec<String>, // TODO Use bitflags
    required: bool,
    skip_check: bool,
}

/// Convert type-only props declaration to a runtime value
fn extract_runtime_props(
    ctx: &mut TypeResolveContext,
    type_decl: &TsType,
    defaults: Option<&Expr>,
    bindings_helper: &mut BindingsHelper,
) -> Option<Box<Expr>> {
    let props = resolve_runtime_props_from_type(ctx, type_decl);
    if props.is_empty() {
        return None;
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
        if !bindings_helper.setup_bindings.iter().any(|it| it.0 == key) {
            bindings_helper
                .setup_bindings
                .push(SetupBinding(key, BindingTypes::Props));
        }
    }

    // TODO Implement hasStaticDefaults (ez)

    // TODO
    // if (ctx.propsRuntimeDefaults && !hasStaticDefaults) {
    //   propsDecls = `/*#__PURE__*/${ctx.helper(
    //     'mergeDefaults',
    //   )}(${propsDecls}, ${ctx.getString(ctx.propsRuntimeDefaults)})`

    Some(Box::new(Expr::Object(props_obj)))
}

fn resolve_runtime_props_from_type(
    ctx: &mut TypeResolveContext,
    type_decl: &TsType,
) -> Vec<PropTypeData> {
    todo!()
}

fn get_runtime_prop_from_type(
    ctx: &mut TypeResolveContext,
    prop: PropTypeData,
    defaults: Option<&Expr>,
    has_static_defaults: bool,
) -> PropOrSpread {
    let mut default: Option<Box<Prop>> = None;
    let default_prop_name = PropName::Ident(Ident {
        span: DUMMY_SP,
        sym: fervid_atom!("default"),
        optional: false,
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
                key: PropName::Ident(Ident {
                    span: DUMMY_SP,
                    sym: fervid_atom!($name),
                    optional: false,
                }),
                value: $value,
            }))))
        };
    }

    macro_rules! return_value {
        ($prop_object_fields: ident) => {
            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                // TODO Better span is probably possible (preserve prop name span)
                key: PropName::Ident(Ident {
                    span: DUMMY_SP,
                    sym: key.to_owned(),
                    optional: false,
                }),
                value: Box::new(Expr::Object(ObjectLit {
                    // TODO Better span is probably possible (preserve prop defaults span?)
                    span: DUMMY_SP,
                    props: $prop_object_fields,
                })),
            })))
        };
    }

    if !ctx.is_prod {
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
    if ctx.is_ce
        || prop
            .types
            .iter()
            .any(|el| el == "Boolean" || (default_defined_or_not_static && el == "Function"))
    {
        // e.g. `type: Number`
        add_field!("type", to_runtime_type_string(prop.types));

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
    need_skip_factory: bool,
}
fn gen_destructured_default_value(
    _ctx: &mut TypeResolveContext,
    _key: &str,
) -> Option<GenDestructuredDefaultValueReturn> {
    // TODO
    None
}

fn to_runtime_type_string(types: Vec<String>) -> Box<Expr> {
    todo!()
}