use fervid_core::{
    BuiltinType, ConditionalDynamicSlot, DynamicSlot, DynamicSlotBuild, DynamicSlotConditional,
    DynamicSlotRenderList, ElementKind, ElementNode, FervidAtom, Node, SlotBuild, SlotFlag,
    SlotSource, Slots, StrOrExpr, VSlotDirective, VueDirectives, VueImports, fervid_atom,
};
use fxhash::FxHashSet;
use smallvec::SmallVec;
use swc_core::{
    common::{Span, Spanned},
    ecma::ast::Pat,
};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        collect_vars::collect_variables, core::v_for::finalize_for_parse_result,
        expr_transform::BindingsHelperTransform,
    },
};

pub fn track_slot_scopes(
    ctx: &mut TransformSfcContext,
    node: &mut ElementNode,
    scope_to_use: u32,
) -> bool {
    // Only components and templates are processed in vuejs-core.
    // Fervid separates built-ins as ElementKind::Builtin so Slots need to be ignored
    let should_process = matches!(
        node.tag_type,
        ElementKind::Component | ElementKind::Template
    ) || matches!(node.tag_type, ElementKind::Builtin(builtin) if !matches!(builtin, BuiltinType::Slot));

    if !should_process {
        return false;
    }

    let Some(v_slot) = node
        .starting_tag
        .directives
        .as_mut()
        .and_then(|v| v.v_slot.as_mut())
    else {
        return false;
    };

    ctx.directive_scopes.v_slot += 1;

    if let Some(v_slot_value) = v_slot.value.as_mut() {
        let scope = &mut ctx.bindings_helper.template_scopes[scope_to_use as usize];
        collect_variables(v_slot_value, scope);
    }

    // Transform `v-slot` argument if it is dynamic
    if let Some(StrOrExpr::Expr(expr)) = v_slot.slot_name.as_mut() {
        ctx.bindings_helper.transform_expr(expr, scope_to_use);
    }

    true
}

pub fn track_v_for_slot_scopes(
    ctx: &mut TransformSfcContext,
    node: &mut ElementNode,
    parent_scope: u32,
    scope_to_use: u32,
) -> bool {
    if !matches!(node.tag_type, ElementKind::Template) {
        return false;
    }

    let Some(directives) = node.starting_tag.directives.as_mut() else {
        return false;
    };

    if directives.v_slot.is_none() {
        return false;
    }

    let Some(v_for) = directives.v_for.as_mut() else {
        return false;
    };

    finalize_for_parse_result(ctx, &mut v_for.parse_result, parent_scope);

    let scope = &mut ctx.bindings_helper.template_scopes[scope_to_use as usize];
    collect_variables(&v_for.parse_result.value, scope);
    if let Some(key) = &v_for.parse_result.key {
        collect_variables(key, scope);
    }
    if let Some(index) = &v_for.parse_result.index {
        collect_variables(index, scope);
    }

    true
}

// Instead of being a DirectiveTransform, v-slot processing is called during
// transformElement to build the slots object for a component.
// Adapted from https://github.com/vuejs/core/blob/5a8aa0b2ba575e098cbb63b396e9bcb751eb3a0f/packages/compiler-core/src/transforms/vSlot.ts#L116-L423
pub fn build_slots(node: &ElementNode, ctx: &mut TransformSfcContext) -> Slots {
    ctx.bindings_helper.helper(VueImports::WithCtx);

    let children = &node.children;
    let mut slots_properties: Vec<SlotBuild> = vec![];
    let mut dynamic_slots: Vec<DynamicSlot> = vec![];

    // If the slot is inside a v-for or another v-slot, force it to be dynamic
    // since it likely uses a scope variable.
    let mut has_dynamic_slots = ctx.directive_scopes.v_slot > 0 || ctx.directive_scopes.v_for > 0;

    // with `prefixIdentifiers: true`, this can be further optimized to make
    // it dynamic when
    // 1. the slot arg or exp uses the scope variables.
    // 2. the slot children use the scope variables.
    // TODO: Port hasScopeRef equivalent for Fervid template scopes.

    // 1. Check for slot with slotProps on component itself.
    //    <Comp v-slot="{ prop }"/>
    let on_component_slot = find_v_slot_on_node(node);
    if let Some(on_component_slot) = on_component_slot {
        let slot_name = on_component_slot.slot_name.as_ref();
        if slot_name.is_some_and(|arg| !is_static_exp(arg)) {
            has_dynamic_slots = true;
        }

        slots_properties.push(SlotBuild {
            name: slot_name
                .cloned()
                .unwrap_or_else(|| StrOrExpr::Str(fervid_atom!("default"))),
            props: on_component_slot.value.clone(),
            source: SlotSource::ImplicitDefaultSlot((0..children.len()).collect()),
        });
    }

    // 2. Iterate through children and check for template slots
    //    <template v-slot:foo="{ prop }">
    let mut has_template_slots = false;
    let mut has_named_default_slot = false;
    let mut implicit_default_children = SmallVec::<[usize; 1]>::new();
    let mut seen_slot_names = FxHashSet::<FervidAtom>::default();
    let mut conditional_branch_index = 0;

    for (i, slot_element) in children.iter().enumerate() {
        let Some((slot_dir, slot_element_node, slot_element_directives)) =
            find_template_slot(slot_element)
        else {
            // not a <template v-slot>, skip.
            if !matches!(slot_element, Node::Comment(_, _)) {
                implicit_default_children.push(i);
            }
            continue;
        };

        if on_component_slot.is_some() {
            // already has on-component slot - this is incorrect usage.
            push_error(
                ctx,
                slot_element_node.span,
                TemplateErrorKind::VSlotMixedSlotUsage,
            );
            break;
        }

        has_template_slots = true;
        // const { children: slotChildren, loc: slotLoc } = slotElement
        // Fervid codegen resolves TemplateSlotChildren(i) back to slotElement.children.
        let slot_name = slot_dir
            .slot_name
            .clone()
            .unwrap_or_else(|| StrOrExpr::Str(fervid_atom!("default")));
        let slot_props = slot_dir.value.clone();

        // check if name is dynamic.
        let mut static_slot_name = None;
        if let Some(slot_name) = get_static_exp(&slot_name) {
            static_slot_name = Some(slot_name);
        } else {
            has_dynamic_slots = true;
        }

        let v_for = slot_element_directives.v_for.as_ref();
        // const slotFunction = buildSlotFn(slotProps, vFor, slotChildren, slotLoc)
        // Fervid delays slot function codegen; SlotBuild carries slotProps and source.

        // check if this slot is conditional (v-if/v-for)
        let v_if = slot_element_directives.v_if.as_ref();
        let v_else = slot_element_directives
            .v_else_if
            .as_ref()
            .map(|_| ())
            .or(slot_element_directives.v_else.map(|_| ()));
        if let Some(condition) = v_if {
            has_dynamic_slots = true;
            dynamic_slots.push(DynamicSlot::Conditional(DynamicSlotConditional {
                if_slot: ConditionalDynamicSlot {
                    condition: condition.to_owned(),
                    slot: build_dynamic_slot(
                        slot_name,
                        slot_props,
                        i,
                        Some(conditional_branch_index),
                    ),
                },
                else_if_slots: vec![],
                else_slot: None,
            }));
            conditional_branch_index += 1;
            continue;
        } else if v_else.is_some() {
            // find adjacent v-if
            let mut j = i;
            let mut prev = None;
            while j > 0 {
                j -= 1;
                let candidate = &children[j];
                if !is_comment_or_whitespace(candidate) {
                    prev = Some(candidate);
                    break;
                }
            }

            if prev.is_some_and(is_template_with_if) {
                let Some(DynamicSlot::Conditional(conditional)) = dynamic_slots.last_mut() else {
                    // Vue asserts this in tests; keep a transform error in Fervid for resilience.
                    push_error(
                        ctx,
                        slot_element_node.span,
                        TemplateErrorKind::VElseNoAdjacentIf,
                    );
                    continue;
                };

                let slot =
                    build_dynamic_slot(slot_name, slot_props, i, Some(conditional_branch_index));

                if let Some(condition) = slot_element_directives.v_else_if.clone() {
                    conditional
                        .else_if_slots
                        .push(ConditionalDynamicSlot { condition, slot });
                } else {
                    conditional.else_slot = Some(slot);
                }

                conditional_branch_index += 1;
            } else {
                push_error(
                    ctx,
                    slot_element_node.span,
                    TemplateErrorKind::VElseNoAdjacentIf,
                );
            }
            continue;
        } else if v_for.is_some() {
            has_dynamic_slots = true;
            dynamic_slots.push(DynamicSlot::RenderList(DynamicSlotRenderList {
                slot_template_index: i,
                slot: build_dynamic_slot(slot_name, slot_props, i, None),
            }));
            continue;
        } else {
            // check duplicate static names
            if let Some(static_slot_name) = static_slot_name {
                if seen_slot_names.contains(&static_slot_name) {
                    push_error(
                        ctx,
                        slot_element_node.span,
                        TemplateErrorKind::VSlotDuplicateSlotNames,
                    );
                    continue;
                }
                seen_slot_names.insert(static_slot_name.to_owned());
                if static_slot_name == "default" {
                    has_named_default_slot = true;
                }
            }
            slots_properties.push(SlotBuild {
                name: slot_name,
                props: slot_props,
                source: SlotSource::TemplateSlotChildren(i),
            });
        }
    }

    if on_component_slot.is_none() {
        if !has_template_slots {
            // implicit default slot (on component)
            build_default_slot_property(&mut slots_properties, implicit_default_children);
        } else if !implicit_default_children.is_empty()
            // #3766
            // with whitespace: 'preserve', whitespaces between slots will end up in
            // implicitDefaultChildren. Ignore if all implicit children are whitespaces.
            && !implicit_default_children
                .iter()
                .all(|idx| is_whitespace_text(&children[*idx]))
        {
            // implicit default slot (mixed with named slots)
            if has_named_default_slot {
                push_error(
                    ctx,
                    children[implicit_default_children[0]].span(),
                    TemplateErrorKind::VSlotExtraneousDefaultSlotChildren,
                );
            } else {
                build_default_slot_property(&mut slots_properties, implicit_default_children);
            }
        }
    }

    let slot_flag = if has_dynamic_slots {
        SlotFlag::Dynamic
    } else if has_forwarded_slots(children) {
        SlotFlag::Forwarded
    } else {
        SlotFlag::Stable
    };

    // Codegen will wrap the static slots object in createSlots(...) when dynamicSlots is not empty.

    Slots {
        slots: slots_properties,
        dynamic_slots,
        has_dynamic_slots,
        slot_flag,
    }
}

fn build_dynamic_slot(
    name: StrOrExpr,
    props: Option<Box<Pat>>,
    slot_template_index: usize,
    key: Option<usize>,
) -> DynamicSlotBuild {
    DynamicSlotBuild {
        name,
        props,
        source: SlotSource::TemplateSlotChildren(slot_template_index),
        key,
    }
}

fn push_error(ctx: &mut TransformSfcContext, span: Span, kind: TemplateErrorKind) {
    ctx.errors
        .push(TransformError::TemplateError(TemplateError { span, kind }));
}

fn build_default_slot_property(
    slots_properties: &mut Vec<SlotBuild>,
    children: SmallVec<[usize; 1]>,
) {
    slots_properties.push(SlotBuild {
        name: StrOrExpr::Str(fervid_atom!("default")),
        // This function is called for implicit default slot
        props: None,
        source: SlotSource::ImplicitDefaultSlot(children),
    });
}

fn find_v_slot_on_node(node: &ElementNode) -> Option<&VSlotDirective> {
    node.starting_tag
        .directives
        .as_deref()
        .and_then(|directives| directives.v_slot.as_ref())
}

fn find_template_slot(node: &Node) -> Option<(&VSlotDirective, &ElementNode, &VueDirectives)> {
    let Node::Element(element) = node else {
        return None;
    };

    if !is_template_node(element) {
        return None;
    }

    let directives = element.starting_tag.directives.as_deref()?;
    let v_slot = directives.v_slot.as_ref()?;

    Some((v_slot, element, directives))
}

fn is_template_node(node: &ElementNode) -> bool {
    node.starting_tag.tag_name == "template"
}

fn is_template_with_if(node: &Node) -> bool {
    let Node::Element(element) = node else {
        return false;
    };

    is_template_node(element)
        && element
            .starting_tag
            .directives
            .as_ref()
            .is_some_and(|directives| directives.v_if.is_some() || directives.v_else_if.is_some())
}

fn is_comment_or_whitespace(node: &Node) -> bool {
    matches!(node, Node::Comment(_, _)) || is_whitespace_text(node)
}

fn is_whitespace_text(node: &Node) -> bool {
    matches!(node, Node::Text(text, _) if text.trim().is_empty())
}

fn is_static_exp(arg: &StrOrExpr) -> bool {
    match arg {
        StrOrExpr::Str(_) => true,
        StrOrExpr::Expr(expr) => expr.is_lit(),
    }
}
fn get_static_exp(arg: &StrOrExpr) -> Option<FervidAtom> {
    match arg {
        StrOrExpr::Str(s) => Some(s.to_owned()),
        StrOrExpr::Expr(expr) => expr
            .as_lit()
            .and_then(|v| v.as_str())
            .map(|v| v.value.to_owned()),
    }
}

fn has_forwarded_slots(children: &[Node]) -> bool {
    children.iter().any(has_forwarded_slots_in_node)
}

fn has_forwarded_slots_in_node(node: &Node) -> bool {
    match node {
        Node::Element(element) => {
            matches!(element.tag_type, ElementKind::Builtin(BuiltinType::Slot))
                || has_forwarded_slots(&element.children)
        }
        Node::For(for_node) => has_forwarded_slots(&for_node.children),
        Node::ConditionalSeq(conditional) => {
            has_forwarded_slots_in_node(&conditional.if_node.node)
                || conditional
                    .else_if_nodes
                    .iter()
                    .any(|branch| has_forwarded_slots_in_node(&branch.node))
                || conditional
                    .else_node
                    .as_deref()
                    .is_some_and(has_forwarded_slots_in_node)
        }
        Node::Text(_, _) | Node::Interpolation(_) | Node::Comment(_, _) => false,
    }
}
