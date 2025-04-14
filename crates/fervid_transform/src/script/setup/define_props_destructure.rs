use fervid_core::{BindingTypes, FervidAtom};
use swc_core::{
    common::{Spanned, DUMMY_SP},
    ecma::ast::{Expr, ObjectPat, ObjectPatProp, Pat},
};

use crate::{
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::{resolve_type::TypeResolveContext, utils::resolve_object_key},
    PropsDestructureBinding, PropsDestructureConfig, SetupBinding,
};

// TODO This is a difference with the official compiler:
// - official compiler does separate collection (called `process`) and processing (called `extract`) loops;
// - it works fine because AST is not really being manipulated and instead being referenced everywhere;
// Fervid cannot afford this, because it does collection-processing on the same loop
// Fervid can still do pre-processing though by collecting variables before the main processing
// For props destructure it means collecting bindings with their default values and then doing a separate loop to transform (not necessarily in pre-processing stage, but before the main processing)

pub fn process_props_destructure(ctx: &mut TypeResolveContext, errors: &mut Vec<TransformError>) {}

pub fn collect_props_destructure(
    ctx: &mut TypeResolveContext,
    obj_pat: &ObjectPat,
    errors: &mut Vec<TransformError>,
) {
    match ctx.props_destructure {
        PropsDestructureConfig::False => return,
        PropsDestructureConfig::True => {}
        PropsDestructureConfig::Error => {
            errors.push(TransformError::ScriptError(ScriptError {
                span: DUMMY_SP, // TODO
                kind: ScriptErrorKind::DefinePropsDestructureForbidden,
            }));
        }
    }

    /// https://github.com/vuejs/core/blob/466b30f4049ec89fb282624ec17d1a93472ab93f/packages/compiler-sfc/src/script/definePropsDestructure.ts#L39
    fn register_binding(
        ctx: &mut TypeResolveContext,
        key: &FervidAtom,
        local: &FervidAtom,
        default_value: Option<Box<Expr>>,
    ) {
        if local != key {
            ctx.bindings_helper
                .setup_bindings
                .push(SetupBinding::new_spanned(local.to_owned(), BindingTypes::PropsAliased, Span::new(BytePos(0), BytePos(0))));

            ctx.bindings_helper
                .props_aliases
                .insert(local.to_owned(), key.to_owned());
        }

        ctx.bindings_helper.props_destructured_bindings.insert(
            key.to_owned(),
            PropsDestructureBinding {
                local: local.to_owned(),
                default: default_value,
            },
        );
    }

    // https://github.com/vuejs/core/blob/466b30f4049ec89fb282624ec17d1a93472ab93f/packages/compiler-sfc/src/script/definePropsDestructure.ts#L52-L89
    for prop in obj_pat.props.iter() {
        match prop {
            // Covers `const { foo: bar } = defineProps()`
            ObjectPatProp::KeyValue(key_value_pat_prop) => {
                let Some(prop_key) = resolve_object_key(&key_value_pat_prop.key) else {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: key_value_pat_prop.span(),
                        kind: ScriptErrorKind::DefinePropsDestructureCannotUseComputedKey,
                    }));
                    continue;
                };

                let Pat::Ident(ident) = key_value_pat_prop.value.as_ref() else {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: key_value_pat_prop.span(),
                        kind: ScriptErrorKind::DefinePropsDestructureUnsupportedNestedPattern,
                    }));
                    continue;
                };

                register_binding(ctx, &prop_key, &ident.sym, None);
            }

            // Covers `const { foo = bar }` and `const { foo }`
            ObjectPatProp::Assign(assign_pat_prop) => {
                let prop_key = &assign_pat_prop.key.sym;
                register_binding(ctx, prop_key, prop_key, assign_pat_prop.value.to_owned());
            }

            // Covers `rest` property in `const { foo, ...rest }`
            ObjectPatProp::Rest(rest_pat) => {
                let Some(rest_pat_name) = rest_pat.arg.as_ident() else {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: rest_pat.span,
                        kind: ScriptErrorKind::DefinePropsDestructureUnsupportedNestedPattern,
                    }));
                    continue;
                };

                let key = rest_pat_name.id.sym.to_owned();

                ctx.bindings_helper.props_destructured_rest_id = Some(key.to_owned());

                ctx.bindings_helper
                    .setup_bindings
                    .push(SetupBinding::new_spanned(key, BindingTypes::SetupReactiveConst, Span::new(BytePos(0), BytePos(0))));
            }
        }
    }
}
