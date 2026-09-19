use fervid_core::{
    AttributeOrBinding, BuiltinType, ElementCodegenNode, ElementCodegenValue, ElementKind,
    ElementNode, FervidAtom, Node, PropsExpression, SlotOutletCall, StrOrExpr, fervid_atom,
};
use smallvec::SmallVec;
use swc_core::common::DUMMY_SP;

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        node_transforms::TransformNodeState,
        transform_element::build_props,
        utils::{is_static_arg_of, to_camel_case},
    },
};

pub fn pre_transform_slot_outlet(
    ctx: &mut TransformSfcContext,
    state: &mut TransformNodeState,
    node: &mut Node,
) {
    let Node::Element(element_node) = node else {
        return;
    };

    if !matches!(
        element_node.tag_type,
        ElementKind::Builtin(BuiltinType::Slot)
    ) {
        return;
    }

    let SlotOutletProcessResult {
        slot_name,
        slot_props,
    } = process_slot_outlet(ctx, state, element_node);

    element_node.codegen_node = Some(Box::new(ElementCodegenNode {
        value: ElementCodegenValue::SlotOutletCall(Box::new(SlotOutletCall {
            name: slot_name,
            props: slot_props,
            fallback: if element_node.children.is_empty() {
                fervid_core::SlotOutletFallback::None
            } else {
                fervid_core::SlotOutletFallback::UseChildren
            },
            span: element_node.span,
        })),
        cache: Default::default(),
    }));
}

pub struct SlotOutletProcessResult {
    slot_name: StrOrExpr,
    slot_props: Option<PropsExpression>,
}

pub fn process_slot_outlet(
    ctx: &mut TransformSfcContext,
    state: &mut TransformNodeState,
    node: &mut ElementNode,
) -> SlotOutletProcessResult {
    let mut slot_name = StrOrExpr::Str(fervid_atom!("default").into());

    let mut skipped_attribute_indices = SmallVec::<[usize; 1]>::new();

    for (index, attr_or_binding) in node.starting_tag.attributes.iter_mut().enumerate() {
        match attr_or_binding {
            AttributeOrBinding::RegularAttribute { value: None, .. } => {
                skipped_attribute_indices.push(index);
            }

            AttributeOrBinding::RegularAttribute {
                name,
                value: Some(value),
                ..
            } => {
                if name == "name" {
                    skipped_attribute_indices.push(index);
                    slot_name = StrOrExpr::Str(value.to_owned().into());
                } else {
                    let mut camelized = String::with_capacity(name.len());
                    to_camel_case(name, &mut camelized);
                    *name = FervidAtom::from(camelized);
                }
            }

            AttributeOrBinding::VBind(v_bind_directive)
                if is_static_arg_of(v_bind_directive.argument.as_ref(), "name") =>
            {
                // Note: here vuejs-core also expands `:name` to `:name="name"`
                // and processes it. Fervid, however, expands props in parser already.
                skipped_attribute_indices.push(index);
                slot_name = StrOrExpr::Expr(v_bind_directive.value.to_owned());
            }

            AttributeOrBinding::VBind(v_bind_directive) => {
                if let Some(StrOrExpr::Str(str_argument)) = v_bind_directive.argument.as_mut() {
                    let mut camelized = String::with_capacity(str_argument.value.len());
                    to_camel_case(&str_argument.value, &mut camelized);
                    str_argument.value = FervidAtom::from(camelized);
                }
            }

            // We are not adding other cases to a vec since `build_props` can simply ignore the indices instead.
            _ => {}
        }
    }

    let build_props_result = build_props(
        node,
        ctx,
        state,
        &skipped_attribute_indices,
        false,
        false,
        false,
    );

    if let Some(_illegal_directive) = build_props_result.directives.first() {
        // TODO Use span of illegal_directive
        let span = DUMMY_SP;

        ctx.errors
            .push(TransformError::TemplateError(TemplateError {
                span,
                kind: TemplateErrorKind::VSlotUnexpectedDirectiveOnSlotOutlet,
            }));
    }

    SlotOutletProcessResult {
        slot_name,
        slot_props: build_props_result.props,
    }
}
