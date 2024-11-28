use swc_core::common::DUMMY_SP;

use crate::{error::{ScriptError, TransformError}, script::resolve_type::TypeResolveContext, PropsDestructureConfig};

pub fn process_props_destructure(
    ctx: &mut TypeResolveContext,
    errors: &mut Vec<TransformError>
) {
    match ctx.props_destructure {
        PropsDestructureConfig::False => return,
        PropsDestructureConfig::True => {},
        PropsDestructureConfig::Error => {
            errors.push(TransformError::ScriptError(ScriptError {
                span: DUMMY_SP, // TODO
                kind: crate::error::ScriptErrorKind::DefinePropsDestructureForbidden,
            }));
        },
    }
}