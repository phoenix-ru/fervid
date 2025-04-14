use std::mem;

use fervid_core::{fervid_atom, BindingTypes, FervidAtom, VueImports};
use itertools::Itertools;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        Bool, CallExpr, Callee, Expr, ExprOrSpread, Ident, IdentName, KeyValueProp, Lit, ObjectLit,
        Prop, PropName, PropOrSpread, SpreadElement, Str, TsTypeParamInstantiation,
    },
};

use crate::{
    atoms::{MODEL_VALUE, PROPS_HELPER, USE_MODEL_HELPER},
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::resolve_type::{infer_runtime_type_type, TypeResolveContext, Types, TypesSet},
    BindingsHelper, SetupBinding, SfcDefineModel, SfcExportedObjectHelper,
};

use super::{macros::TransformMacroResult, utils::to_runtime_type_string};

pub fn process_define_model(
    call_expr: &CallExpr,
    is_var_decl: bool,
    is_ident: bool,
    var_bindings: Option<&mut Vec<SetupBinding>>,
    sfc_object_helper: &mut SfcExportedObjectHelper,
    bindings_helper: &mut BindingsHelper,
) -> TransformMacroResult {
    let mut define_model = read_define_model(&call_expr.args, call_expr.type_args.as_deref());
    let span = call_expr.span;

    // Check duplicate
    if sfc_object_helper
        .models
        .iter()
        .any(|v| v.name.value == define_model.name.value)
    {
        return TransformMacroResult::Error(TransformError::ScriptError(ScriptError {
            span,
            kind: ScriptErrorKind::DuplicateDefineModelName,
        }));
    }

    // Add to imports
    bindings_helper.vue_imports |= VueImports::UseModel;

    let use_model_ident = Ident {
        span,
        ctxt: Default::default(),
        sym: USE_MODEL_HELPER.to_owned(),
        optional: false,
    };

    let mut use_model_args =
        Vec::<ExprOrSpread>::with_capacity(if define_model.use_model_options.is_some() {
            3
        } else {
            2
        });

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
        expr: Box::new(Expr::Lit(Lit::Str(model_name.to_owned()))),
    });

    // Add `useModel` options
    if let Some(use_model_options) = define_model.use_model_options.take() {
        use_model_args.push(*use_model_options)
    }

    // Get type args
    let type_args = define_model.ts_type.as_ref().map(|ts_type| {
        Box::new(TsTypeParamInstantiation {
            span,
            params: vec![Box::new(ts_type.to_owned())],
        })
    });

    sfc_object_helper.models.push(define_model);

    // Binding type of the prop
    bindings_helper
        .setup_bindings
        .push(SetupBinding::new_spanned(model_name.value, BindingTypes::Props, model_name.span));

    // Binding type of the model itself
    if let (true, true, Some(var_bindings)) = (is_var_decl, is_ident, var_bindings) {
        if var_bindings.len() == 1 {
            let binding = &mut var_bindings[0];
            binding.binding_type = BindingTypes::SetupRef;
        }
    }

    // _useModel(__props, "model-name", %model options%)
    TransformMacroResult::ValidMacro(Some(Box::new(Expr::Call(CallExpr {
        span,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(use_model_ident))),
        args: use_model_args,
        type_args,
    }))))
}

/// Processes `defineModel`
fn read_define_model(
    macro_args: &[ExprOrSpread],
    type_args: Option<&TsTypeParamInstantiation>,
) -> SfcDefineModel {
    // 1st arg - model name (string) or model options (object)
    let first_arg = macro_args.get(0);

    // 2nd arg - model options (object)
    let second_arg = macro_args.get(1);

    macro_rules! default_model_name {
        () => {
            Str {
                span: DUMMY_SP,
                value: MODEL_VALUE.to_owned(),
                raw: None,
            }
        };
    }

    // Get name. It may be a first argument, or may be omitted altogether (defaults to `modelValue`)
    let (name, is_first_arg_name) = match first_arg {
        Some(ExprOrSpread { spread: None, expr }) => match **expr {
            Expr::Lit(Lit::Str(ref name)) => (name.to_owned(), true),
            _ => (default_model_name!(), false),
        },

        _ => (default_model_name!(), false),
    };

    let options: Option<&ExprOrSpread> = if is_first_arg_name {
        second_arg
    } else {
        first_arg
    };

    // Distinguish between `useModel` vs prop options
    let mut prop_options: Option<Box<Expr>> = None;
    let mut use_model_options: Option<Box<ExprOrSpread>> = None;
    if let Some(options) = options {
        if let Some(options) = read_options(options) {
            if !options.0.is_empty() {
                prop_options = Some(Box::new(Expr::Object(ObjectLit {
                    span: DUMMY_SP,
                    props: options.0,
                })));
            }
            if !options.1.is_empty() {
                use_model_options = Some(Box::new(ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Object(ObjectLit {
                        span: DUMMY_SP,
                        props: options.1,
                    })),
                }));
            }
        } else {
            // Options defined by user but follow a non-trivial format (computed or spread), use as-is.
            // When user options include a spread, we cannot pass it to props - we pass nothing instead.
            prop_options = if options.spread.is_none() {
                Some(options.expr.clone())
            } else {
                None
            };
            use_model_options = Some(Box::new(options.clone()));
        }
    }

    SfcDefineModel {
        name,
        prop_options,
        use_model_options,
        ts_type: type_args
            .and_then(|v| v.params.first())
            .map(|v| (**v).to_owned()),
    }
}

/// Splits options passed to `defineModel` into `useModel`-specific ones vs prop-specific
fn read_options(options: &ExprOrSpread) -> Option<(Vec<PropOrSpread>, Vec<PropOrSpread>)> {
    // Only object expression is supported
    let ExprOrSpread { spread: None, expr } = options else {
        return None;
    };

    let Expr::Object(ref obj) = **expr else {
        return None;
    };

    let mut prop_options = Vec::with_capacity(obj.props.len());
    let mut use_model_options = Vec::with_capacity(2);

    fn get_ident_computed(v: &PropName) -> (Option<&swc_core::atoms::Atom>, bool) {
        match v {
            PropName::Ident(ident_name) => (Some(&ident_name.sym), false),
            PropName::Str(s) => (Some(&s.value), false),
            PropName::Num(_) => (None, false),
            PropName::Computed(_) => (None, true),
            PropName::BigInt(_) => (None, false),
        }
    }

    for prop_or_spread in obj.props.iter() {
        let PropOrSpread::Prop(prop) = prop_or_spread else {
            // Any spread automatically means options need to be duplicated
            return None;
        };

        let (ident, computed) = match prop.as_ref() {
            Prop::Shorthand(ident) => (Some(&ident.sym), false),
            Prop::KeyValue(key_value_prop) => get_ident_computed(&key_value_prop.key),
            Prop::Assign(_) => return None,
            Prop::Getter(getter_prop) => get_ident_computed(&getter_prop.key),
            Prop::Setter(setter_prop) => get_ident_computed(&setter_prop.key),
            Prop::Method(method_prop) => get_ident_computed(&method_prop.key),
        };

        // Also with computed - options need to be duplicated
        if computed {
            return None;
        }

        // `get` and `set` properties are `useModel`-specific
        if (prop.is_shorthand() || prop.is_key_value() || prop.is_method())
            && ident.is_some_and(|v| v == "set" || v == "get")
        {
            use_model_options.push(prop_or_spread);
        } else {
            prop_options.push(prop_or_spread);
        }
    }

    Some((
        prop_options.drain(..).cloned().collect_vec(),
        use_model_options.drain(..).cloned().collect_vec(),
    ))
}

pub fn postprocess_models(
    ctx: &mut TypeResolveContext,
    models: &mut Vec<SfcDefineModel>,
    props: &mut Vec<PropOrSpread>,
    emits: &mut Vec<Option<ExprOrSpread>>,
) {
    if models.is_empty() {
        return;
    }

    let scope = ctx.root_scope();
    let scope = &*scope.borrow();
    for model in models.drain(..) {
        let has_prop_options = model.prop_options.is_some();
        let mut model_value: Box<Expr> = match model.prop_options {
            Some(options) => options,
            None => Box::new(Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: vec![],
            })),
        };

        // https://github.com/vuejs/core/blob/a0901756dad225c1addf54dab9a297801a9c8c1a/packages/compiler-sfc/src/script/defineModel.ts#L125-L153
        let mut skip_check = false;
        let mut codegen_options = Vec::<PropOrSpread>::new();
        let runtime_types = model
            .ts_type
            .map(|v| infer_runtime_type_type(ctx, &v, scope, false));

        if let Some(mut runtime_types) = runtime_types {
            let has_boolean = runtime_types.contains(Types::Boolean);
            let has_function = runtime_types.contains(Types::Function);
            let has_unknown_type = runtime_types.contains(Types::Unknown);

            if has_unknown_type {
                if has_boolean || has_function {
                    runtime_types -= Types::Unknown;
                    skip_check = true;
                } else {
                    runtime_types = TypesSet::from(Types::Null);
                }
            }

            if !ctx.bindings_helper.is_prod {
                codegen_options.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(IdentName {
                        span: DUMMY_SP,
                        sym: fervid_atom!("type"),
                    }),
                    value: to_runtime_type_string(runtime_types),
                }))));

                if skip_check {
                    codegen_options.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
                        KeyValueProp {
                            key: PropName::Ident(IdentName {
                                span: DUMMY_SP,
                                sym: fervid_atom!("skipCheck"),
                            }),
                            value: Box::new(Expr::Lit(Lit::Bool(Bool {
                                span: DUMMY_SP,
                                value: true,
                            }))),
                        },
                    ))));
                }
            } else if has_boolean || (has_function && has_prop_options) {
                codegen_options.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(IdentName {
                        span: DUMMY_SP,
                        sym: fervid_atom!("type"),
                    }),
                    value: to_runtime_type_string(runtime_types),
                }))));
            }
        }

        if !codegen_options.is_empty() {
            if let Expr::Object(ref mut options_obj) = &mut *model_value {
                options_obj.props.extend(codegen_options.drain(..))
            } else {
                // Surround the existing with spread
                // We end up in `{ type: [Foo], ...userOptions }`
                let old_value = mem::replace(
                    &mut model_value,
                    Box::new(Expr::Object(ObjectLit {
                        span: DUMMY_SP,
                        props: codegen_options,
                    })),
                );
                model_value
                    .as_mut_object()
                    .unwrap()
                    .props
                    .push(PropOrSpread::Spread(SpreadElement {
                        dot3_token: DUMMY_SP,
                        expr: old_value,
                    }));
            }
        }

        let model_update_evt_name = format!("update:{}", &model.name.value);

        // Push a string literal into emits
        emits.push(Some(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: FervidAtom::from(model_update_evt_name),
                raw: None,
            }))),
        }));

        // For modifiers
        let modifier_name = if &model.name.value == "modelValue" {
            fervid_atom!("modelModifiers")
        } else {
            FervidAtom::from(format!("{}Modifiers", &model.name.value))
        };

        // Push an options object (or expr) into props
        props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Str(model.name),
            value: model_value,
        }))));

        // Push the modifiers prop
        props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Str(Str {
                span: DUMMY_SP,
                value: modifier_name,
                raw: None,
            }),
            value: Box::new(Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: vec![],
            })),
        }))));
    }
}
