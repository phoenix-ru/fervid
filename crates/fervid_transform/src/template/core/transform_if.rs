use fervid_core::{
    AttributeOrBinding, BuiltinType, Conditional, ConditionalNodeSequence, ConstantTypes,
    ElementCodegenNode, ElementCodegenValue, ElementKind, ElementNode, ExpressionNode,
    ExpressionPropNameNode, Node, PatchFlags, PatchHints, Property, SimpleExpressionNode,
    StartingTag, StrOrExpr, VNodeCall, VNodeCallTag, VNodeChildren,
    create_simple_expression_propname, fervid_atom,
};
use smallvec::SmallVec;
use swc_core::{
    common::{DUMMY_SP, EqIgnoreSpan, Span, Spanned},
    ecma::ast::{Expr, Lit, Number},
};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        expr_transform::BindingsHelperTransform,
        utils::{has_unnormalized_prop, inject_prop, is_static_arg_of},
    },
    utils::{convert_to_block, unwrap_ts_node_expr},
};

pub fn transform_if(
    ctx: &mut TransformSfcContext,
    children: &mut Vec<Node>,
    parent_kind: ElementKind,
    scope_to_use: u32,
) {
    if children.is_empty() {
        return;
    }

    // Merge multiple v-if/else-if/else nodes into a ConditionalNodeSequence
    let mut seq: Option<ConditionalNodeSequence> = None;
    let mut new_children = Vec::with_capacity(children.len());
    let mut branch_keys = SmallVec::<[BranchKeySource; 4]>::new();

    // Comment processing
    let mut comments = Vec::<Node>::new();
    let is_dev = !ctx.bindings_helper.is_prod;
    let is_parent_not_transition =
        !matches!(parent_kind, ElementKind::Builtin(BuiltinType::Transition));

    // Auto-incremented conditional node indices
    let mut next_key_base = 0;

    /// Finishes the sequence. Pass `child` to also push the current child
    macro_rules! finish_seq {
        () => {
            if let Some(seq) = seq.take() {
                validate_branch_keys(ctx, &seq, &branch_keys);
                branch_keys.clear();
                next_key_base += 1 + seq.else_if_nodes.len() + (seq.else_node.is_some() as usize);
                new_children.push(Node::ConditionalSeq(seq))
            }
        };
        ($child: expr) => {
            finish_seq!();
            new_children.push($child);
        };
    }

    // To move out of &ElementNode to ElementNode and avoid "partially moved variable" error
    macro_rules! deref_element {
        ($child: ident) => {{
            let Node::Element(child_element) = $child else {
                // SAFETY - This loop below only works on ElementNodes
                unreachable!()
            };

            Node::Element(optimize_v_if_plus_v_for(child_element))
        }};
    }

    for mut child in children.drain(..) {
        // Only process `ElementNode`s.
        // Otherwise, when we have an `if` node, ignore `Comment`s and finish sequence.
        let child_element = match child {
            Node::Element(ref mut element_node) => element_node,
            Node::Comment(_, _) => {
                if seq.is_some() && is_dev && is_parent_not_transition {
                    comments.push(child);
                } else if seq.is_none() {
                    new_children.push(child);
                }
                continue;
            }
            child => {
                finish_seq!(child);
                continue;
            }
        };

        // `<template v-slot>` structural directives belong to build_slots
        if is_template_slot_carrier(child_element) {
            if let Some(directives) = child_element.starting_tag.directives.as_mut() {
                if let Some(ref mut v_if_cond) = directives.v_if {
                    ctx.bindings_helper.transform_expr(v_if_cond, scope_to_use);
                }
                if let Some(ref mut v_else_if_cond) = directives.v_else_if {
                    ctx.bindings_helper
                        .transform_expr(v_else_if_cond, scope_to_use);
                }
            }

            finish_seq!(child);
            continue;
        }

        let Some(ref mut directives) = child_element.starting_tag.directives else {
            finish_seq!(child);
            continue;
        };

        // Check if we have a `v-if`.
        // The already existing sequence should end, and the new sequence should start.
        if let Some(v_if) = directives.v_if.take() {
            finish_seq!();
            let span = child_element.span;
            let branch_node = deref_element!(child);
            branch_keys.push(branch_key_source(&branch_node, next_key_base));
            seq = Some(ConditionalNodeSequence {
                if_node: Box::new(Conditional {
                    condition: *v_if,
                    node: branch_node,
                }),
                else_if_nodes: vec![],
                else_node: None,
                span,
                key_base: next_key_base,
            });
            continue;
        }

        // Check for `v-else-if`
        if let Some(v_else_if) = directives.v_else_if.take() {
            let Some(ref mut seq) = seq else {
                // Error: v-else-if without v-if
                ctx.errors
                    .push(TransformError::TemplateError(TemplateError {
                        // TODO Use span of directive, not of Expr
                        span: v_else_if.span(),
                        kind: TemplateErrorKind::VElseNoAdjacentIf,
                    }));
                finish_seq!(child);
                continue;
            };

            // Update sequence end span
            seq.span.hi = child_element.span.hi;

            // Attach comments and potentially wrap the `child`
            let was_template = matches!(child_element.tag_type, ElementKind::Template);
            let had_v_for = directives.v_for.is_some();
            let span = child_element.span;
            let branch_node = attach_branch_comments(
                deref_element!(child),
                &mut comments,
                was_template,
                had_v_for,
                span,
            );
            let key = branch_key_source(&branch_node, seq.key_base + seq.else_if_nodes.len() + 1);
            seq.else_if_nodes.push(Conditional {
                condition: *v_else_if,
                node: branch_node,
            });
            branch_keys.push(key);
            continue;
        }

        // Check for `v-else` and take it
        if directives.v_else.take().is_some() {
            let Some(ref mut seq) = seq else {
                // Error: v-else without v-if
                ctx.errors
                    .push(TransformError::TemplateError(TemplateError {
                        // TODO Give this a span
                        span: DUMMY_SP,
                        kind: TemplateErrorKind::VElseNoAdjacentIf,
                    }));
                finish_seq!(child);
                continue;
            };

            // Update sequence end span
            seq.span.hi = child_element.span.hi;

            // Attach comments and potentially wrap the `child`
            let was_template = matches!(child_element.tag_type, ElementKind::Template);
            let had_v_for = directives.v_for.is_some();
            let span = child_element.span;
            let branch_node = attach_branch_comments(
                deref_element!(child),
                &mut comments,
                was_template,
                had_v_for,
                span,
            );
            let key = branch_key_source(&branch_node, seq.key_base + seq.else_if_nodes.len() + 1);
            seq.else_node = Some(Box::new(branch_node));
            branch_keys.push(key);

            // `else` node always finishes the sequence
            finish_seq!();
            continue;
        }

        // No directives, just push the child
        finish_seq!(child);
    }

    // finish_seq! hand-written to not auto-increment next_key_base
    if let Some(seq) = seq.take() {
        validate_branch_keys(ctx, &seq, &branch_keys);
        new_children.push(Node::ConditionalSeq(seq))
    }

    *children = new_children;
}

pub fn post_transform_if(ctx: &mut TransformSfcContext, node: &mut Node) {
    let Node::ConditionalSeq(seq) = node else {
        return;
    };

    let mut key = seq.key_base;
    inject_branch_key(ctx, &mut seq.if_node.node, key);

    for else_if in seq.else_if_nodes.iter_mut() {
        key += 1;
        inject_branch_key(ctx, &mut else_if.node, key);
    }

    if let Some(ref mut else_node) = seq.else_node {
        key += 1;
        inject_branch_key(ctx, else_node, key);
    }
}

// Optimize combined usage of conditional directives and `v-for`
// https://github.com/vuejs/core/blob/438a74aad840183286fbdb488178510f37218a73/packages/compiler-core/src/transforms/vIf.ts#L260
fn optimize_v_if_plus_v_for(mut parent: ElementNode) -> ElementNode {
    // Check that work is needed
    // This must be a `<template>` element with exactly one Element child
    if parent.children.len() != 1 || parent.starting_tag.tag_name != "template" {
        return parent;
    }

    let Some(Node::Element(child)) = parent.children.first_mut() else {
        return parent;
    };

    // There must be at most one `v-for` for both parent and child
    let parent_has_v_for = parent
        .starting_tag
        .directives
        .as_ref()
        .is_some_and(|d| d.v_for.is_some());
    let child_has_v_for = child
        .starting_tag
        .directives
        .as_ref()
        .is_some_and(|d| d.v_for.is_some());
    if parent_has_v_for && child_has_v_for {
        return parent;
    }

    // Take parent's `v-for` and give it to the child
    if parent_has_v_for {
        let Some(mut parent_directives) = parent.starting_tag.directives.take() else {
            unreachable!()
        };

        let child_directives = child
            .starting_tag
            .directives
            .get_or_insert_with(Default::default);
        child_directives.v_for = parent_directives.v_for.take();
    }

    // Take the child and return it instead
    let Some(Node::Element(child)) = parent.children.pop() else {
        unreachable!()
    };

    child
}

fn attach_branch_comments(
    mut node: Node,
    pending: &mut Vec<Node>,
    was_template: bool,
    had_v_for: bool,
    span: Span,
) -> Node {
    if pending.is_empty() {
        return node;
    }

    let mut children: Vec<Node> = std::mem::take(pending);

    // Original <template v-else> already owns branch children.
    // Prepend comments there and avoid wrapping it
    if was_template
        && !had_v_for
        && let Node::Element(ref mut element) = node
        && matches!(element.tag_type, ElementKind::Template)
    {
        children.append(&mut element.children);
        element.children = children;
        return node;
    }

    children.push(node);
    let mut carrier = ElementNode::new_with_children_and_type(
        StartingTag {
            tag_name: fervid_atom!("template"),
            attributes: vec![],
            directives: None,
        },
        children,
        ElementKind::Template,
    );
    carrier.span = span;
    carrier.patch_hints.flags |= PatchFlags::StableFragment;
    if !was_template {
        carrier.patch_hints.flags |= PatchFlags::DevRootFragment;
    }
    Node::Element(carrier)
}

fn inject_branch_key(ctx: &mut TransformSfcContext, node: &mut Node, key: usize) {
    match node {
        Node::Element(element_node) => {
            let Some(codegen_node) = get_or_create_codegen_node(element_node) else {
                // None is returning when creating is not appropriate (e.g. non-template node).
                debug_assert!(
                    false,
                    "codegen_node could not be created for {:?}",
                    element_node.tag_type
                );
                return;
            };

            match codegen_node.value {
                ElementCodegenValue::VNodeCall(ref mut vnode_call) => {
                    convert_to_block(ctx, vnode_call);
                    inject_prop(
                        &mut vnode_call.props,
                        create_key_prop(key),
                        &mut ctx.bindings_helper,
                    )
                }
                ElementCodegenValue::WithMemoCall(ref mut with_memo_call) => {
                    convert_to_block(ctx, &mut with_memo_call.vnode_call);
                    inject_prop(
                        &mut with_memo_call.vnode_call.props,
                        create_key_prop(key),
                        &mut ctx.bindings_helper,
                    )
                }
                ElementCodegenValue::SlotOutletCall(ref mut slot_outlet_call) => {
                    // Avoid overwriting user `key`
                    if !has_unnormalized_prop(slot_outlet_call.props.as_ref(), "key") {
                        slot_outlet_call.branch_key = Some(Box::new(key.into()));
                    }
                }
            }
        }
        Node::For(for_node) => {
            let Some(codegen_node) = for_node.codegen_node.as_mut() else {
                debug_assert!(false, "For codegen_node was empty");
                return;
            };

            codegen_node.branch_key = Some(key);
        }

        // These branches should not be possible:
        // Node::Text
        // Node::Interpolation
        // Node::Comment
        // Node::ConditionalSeq
        _ => {
            debug_assert!(false, "Unexpected Node type inside Conditional: {:?}", node)
        }
    }
}

fn get_or_create_codegen_node(element_node: &mut ElementNode) -> Option<&mut ElementCodegenNode> {
    if let Some(ref mut value) = element_node.codegen_node {
        return Some(value);
    }

    if !matches!(element_node.tag_type, ElementKind::Template) {
        return None;
    }

    element_node.codegen_node = Some(Box::new(ElementCodegenNode {
        value: ElementCodegenValue::VNodeCall(Box::new(VNodeCall {
            tag: VNodeCallTag::Fragment,
            props: None,
            children: Some(VNodeChildren::UseElementChildren),
            patch_hints: PatchHints {
                flags: element_node.patch_hints.flags | PatchFlags::StableFragment,
                props: vec![],
                should_use_block: false,
            },
            directives: None,
            needs_patch: false,
            is_block_required: false,
            is_block: true,
            disable_tracking: false,
            is_component: false,
        })),
        cache: Default::default(),
    }));
    element_node.codegen_node.as_deref_mut()
}

/// Creates `key: 123` object Property
fn create_key_prop(key: usize) -> Property {
    Property {
        key: ExpressionPropNameNode::SimpleExpression(create_simple_expression_propname(
            fervid_atom!("key"),
            true,
            DUMMY_SP,
        )),
        value: fervid_core::JsChildNode::ExpressionNode(Box::new(
            ExpressionNode::SimpleExpression(SimpleExpressionNode {
                ast: Box::new(Expr::Lit(Lit::Num(Number {
                    span: DUMMY_SP,
                    value: key as f64,
                    raw: None,
                }))),
                is_static: false,
                const_type: ConstantTypes::CanCache,
                is_handler_key: false,
            }),
        )),
        span: DUMMY_SP,
    }
}

fn is_template_slot_carrier(node: &ElementNode) -> bool {
    matches!(node.tag_type, ElementKind::Template)
        && node
            .starting_tag
            .directives
            .as_ref()
            .is_some_and(|directives| directives.v_slot.is_some())
}

#[derive(Clone, Copy)]
enum BranchKeySource {
    Generated(usize),
    Explicit(usize),
    Unknown,
}

fn branch_key_source(node: &Node, generated_key: usize) -> BranchKeySource {
    let Node::Element(element) = node else {
        return BranchKeySource::Generated(generated_key);
    };

    // The outer list Fragment gets the branch key; the per-item key belongs to v-for
    if matches!(element.tag_type, ElementKind::Template)
        || element
            .starting_tag
            .directives
            .as_deref()
            .is_some_and(|directives| directives.v_for.is_some())
    {
        return BranchKeySource::Generated(generated_key);
    }

    // renderSlot computes its Fragment key at runtime, including slot props and fallback
    if matches!(element.tag_type, ElementKind::Builtin(BuiltinType::Slot)) {
        return BranchKeySource::Unknown;
    }

    let attributes = &element.starting_tag.attributes;
    if attributes.iter().any(|attribute| {
        matches!(attribute, AttributeOrBinding::VBind(binding) if binding.argument.is_none() || matches!(binding.argument, Some(StrOrExpr::Expr(_))))
    }) {
        return BranchKeySource::Unknown;
    }

    let user_key = attributes.iter().position(|attribute| match attribute {
        AttributeOrBinding::RegularAttribute { name, .. } => name == "key",
        AttributeOrBinding::VBind(binding) => is_static_arg_of(binding.argument.as_ref(), "key"),
        AttributeOrBinding::VOn(_) => false,
    });

    user_key.map_or(
        BranchKeySource::Generated(generated_key),
        BranchKeySource::Explicit,
    )
}

fn validate_branch_keys(
    ctx: &mut TransformSfcContext,
    seq: &ConditionalNodeSequence,
    keys: &[BranchKeySource],
) {
    for (first_index, first) in keys.iter().enumerate() {
        for (second_index, second) in keys.iter().enumerate().skip(first_index + 1) {
            let first_attribute = explicit_key(seq, first_index, *first);
            let second_attribute = explicit_key(seq, second_index, *second);

            let same_key = match (*first, *second) {
                (BranchKeySource::Generated(a), BranchKeySource::Generated(b)) => a == b,
                (BranchKeySource::Generated(key), BranchKeySource::Explicit(_)) => {
                    second_attribute.is_some_and(|attribute| matches_generated_key(attribute, key))
                }
                (BranchKeySource::Explicit(_), BranchKeySource::Generated(key)) => {
                    first_attribute.is_some_and(|attribute| matches_generated_key(attribute, key))
                }
                (BranchKeySource::Explicit(_), BranchKeySource::Explicit(_)) => first_attribute
                    .zip(second_attribute)
                    .is_some_and(|(a, b)| is_same_key(a, b)),
                _ => false,
            };

            if same_key {
                let span = second_attribute
                    .or(first_attribute)
                    .map(Spanned::span)
                    .unwrap_or(seq.span);
                ctx.errors
                    .push(TransformError::TemplateError(TemplateError {
                        span,
                        kind: TemplateErrorKind::VIfSameKey,
                    }));
            }
        }
    }
}

/// Borrow the AttributeOrBinding from the ConditionalNodeSequence
/// by using multi-meaning `branch_index`:
/// - 0 means to borrow from `if_node`,
/// - 1..else_if_nodes.len() means to borrow from `else_if_nodes` at `branch_index - 1`,
/// - else_if_nodes.len() + 1 means to borrow from `else_node`.
///
/// The `BranchKeySource::Explicit(index)` sets the attribute index inside the node.
fn explicit_key(
    seq: &ConditionalNodeSequence,
    branch_index: usize,
    source: BranchKeySource,
) -> Option<&AttributeOrBinding> {
    let BranchKeySource::Explicit(attribute_index) = source else {
        return None;
    };

    let node = if branch_index == 0 {
        Some(&seq.if_node.node)
    } else if let Some(branch) = seq.else_if_nodes.get(branch_index - 1) {
        Some(&branch.node)
    } else if branch_index == seq.else_if_nodes.len() + 1 {
        seq.else_node.as_deref()
    } else {
        None
    };

    let Some(Node::Element(element)) = node else {
        return None;
    };
    element.starting_tag.attributes.get(attribute_index)
}

fn matches_generated_key(attribute: &AttributeOrBinding, key: usize) -> bool {
    let AttributeOrBinding::VBind(binding) = attribute else {
        return false;
    };

    matches!(unwrap_ts_node_expr(&binding.value), Expr::Lit(Lit::Num(value)) if value.value == key as f64)
}

/// Smart same value detection. Compares attributes, v-bind values,
/// and their combination. v-bind values are unwrapped from the surrounding TS nodes.
/// This is a deeper comparison than vuejs-core exact content matching.
fn is_same_key(a: &AttributeOrBinding, b: &AttributeOrBinding) -> bool {
    match (a, b) {
        (
            AttributeOrBinding::RegularAttribute { value: a_value, .. },
            AttributeOrBinding::RegularAttribute { value: b_value, .. },
        ) => {
            a_value.as_ref().map_or("", |value| value.as_str())
                == b_value.as_ref().map_or("", |value| value.as_str())
        }

        (AttributeOrBinding::VBind(a_binding), AttributeOrBinding::VBind(b_binding)) => {
            unwrap_ts_node_expr(&a_binding.value)
                .eq_ignore_span(unwrap_ts_node_expr(&b_binding.value))
        }

        // Dynamic and regular attribute comparison - only if v-bind value is:
        // a literal string (`'value'),
        // or a TS node wrapping a literal string (e.g. `'value' as Something).
        (
            AttributeOrBinding::RegularAttribute { value, .. },
            AttributeOrBinding::VBind(binding),
        )
        | (
            AttributeOrBinding::VBind(binding),
            AttributeOrBinding::RegularAttribute { value, .. },
        ) => {
            let Expr::Lit(Lit::Str(s)) = unwrap_ts_node_expr(&binding.value) else {
                return false;
            };

            s.value.as_str() == value.as_ref().map_or("", |value| value.as_str())
        }

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{AttributeOrBinding, VBindDirective, VueDirectives};
    #[cfg(feature = "new-pipeline")]
    use fervid_core::{ElementCodegenValue, SfcTemplateBlock};

    use crate::{
        error::{TemplateErrorKind, TransformError},
        template::core::transform_template_attributes::transform_template_attributes,
        test_utils::{element_from_tag, js, to_str},
    };

    use super::*;

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L68-L78
    #[test]
    fn basic_v_if() {
        // <div v-if="ok"/>
        let seq = fold_if(vec![if_branch("div", "ok")]);
        assert_eq!(to_str(&seq.if_node.condition), "ok");
        let Node::Element(element) = &seq.if_node.node else {
            panic!("v-if branch should contain an element");
        };
        assert_eq!(element.starting_tag.tag_name, "div");
        assert!(seq.else_if_nodes.is_empty());
        assert!(seq.else_node.is_none());
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L80-L96
    #[test]
    fn template_v_if() {
        // <template v-if="ok"><div/>hello<p/></template>
        let mut template = branch(
            "template",
            VueDirectives {
                v_if: Some(js("ok")),
                ..Default::default()
            },
        );
        let Node::Element(element) = &mut template else {
            panic!("template branch should contain an element");
        };
        element.tag_type = ElementKind::Template;
        element.children = vec![
            Node::Element(element_from_tag("div")),
            Node::Text("hello".into(), DUMMY_SP),
            Node::Element(element_from_tag("p")),
        ];

        let seq = fold_if(vec![template]);
        let Node::Element(element) = &seq.if_node.node else {
            panic!("template branch should remain an element carrier");
        };
        assert!(matches!(element.tag_type, ElementKind::Template));
        assert_eq!(element.children.len(), 3);
        assert!(
            matches!(&element.children[0], Node::Element(child) if child.starting_tag.tag_name == "div")
        );
        assert!(matches!(&element.children[1], Node::Text(text, _) if text == "hello"));
        assert!(
            matches!(&element.children[2], Node::Element(child) if child.starting_tag.tag_name == "p")
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L98-L114
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn component_v_if() {
        // <Component v-if="ok"></Component>
        let mut component = if_branch("Component", "ok");
        let Node::Element(element) = &mut component else {
            panic!("component branch should contain an element");
        };
        element.tag_type = ElementKind::Component;
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![component],
            span: DUMMY_SP,
        };
        crate::template::transform_and_record_template(
            &mut template,
            &mut TransformSfcContext::anonymous(),
        );

        let [Node::ConditionalSeq(seq)] = template.roots.as_slice() else {
            panic!("component v-if should fold into a conditional");
        };
        let Node::Element(element) = &seq.if_node.node else {
            panic!("component branch should remain an element");
        };
        assert!(matches!(element.tag_type, ElementKind::Component));
        let Some(codegen_node) = element.codegen_node.as_deref() else {
            panic!("component branch should have typed codegen");
        };
        let ElementCodegenValue::VNodeCall(vnode) = &codegen_node.value else {
            panic!("component branch should have a VNode call");
        };
        assert!(vnode.is_block);
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L116-L132
    #[test]
    fn v_if_else() {
        // <div v-if="ok"/><p v-else/>
        let seq = fold_if(vec![
            if_branch("div", "ok"),
            branch(
                "p",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
        ]);
        assert_eq!(to_str(&seq.if_node.condition), "ok");
        assert!(
            matches!(&seq.if_node.node, Node::Element(element) if element.starting_tag.tag_name == "div")
        );
        assert!(
            matches!(seq.else_node.as_deref(), Some(Node::Element(element)) if element.starting_tag.tag_name == "p")
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L134-L152
    #[test]
    fn v_if_else_if() {
        // <div v-if="ok"/><p v-else-if="orNot"/>
        let seq = fold_if(vec![
            if_branch("div", "ok"),
            branch(
                "p",
                VueDirectives {
                    v_else_if: Some(js("orNot")),
                    ..Default::default()
                },
            ),
        ]);
        assert_eq!(to_str(&seq.if_node.condition), "ok");
        let [else_if] = seq.else_if_nodes.as_slice() else {
            panic!("v-else-if should form second branch");
        };
        assert_eq!(to_str(&else_if.condition), "orNot");
        assert!(
            matches!(&else_if.node, Node::Element(element) if element.starting_tag.tag_name == "p")
        );
        assert!(seq.else_node.is_none());
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L154-L178
    #[test]
    fn v_if_else_if_else() {
        // <div v-if="ok"/><p v-else-if="orNot"/><template v-else>fine</template>
        let mut else_template = branch(
            "template",
            VueDirectives {
                v_else: Some(()),
                ..Default::default()
            },
        );
        let Node::Element(element) = &mut else_template else {
            panic!("v-else template should contain an element");
        };
        element.tag_type = ElementKind::Template;
        element.children.push(Node::Text("fine".into(), DUMMY_SP));
        let seq = fold_if(vec![
            if_branch("div", "ok"),
            branch(
                "p",
                VueDirectives {
                    v_else_if: Some(js("orNot")),
                    ..Default::default()
                },
            ),
            else_template,
        ]);
        assert_eq!(seq.else_if_nodes.len(), 1);
        let Some(Node::Element(element)) = seq.else_node.as_deref() else {
            panic!("v-else should retain template carrier");
        };
        assert!(matches!(element.children.as_slice(), [Node::Text(text, _)] if text == "fine"));
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L214-L222
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn should_prefix_v_if_condition() {
        // <div v-if="ok"/>
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![if_branch("div", "ok")],
            span: DUMMY_SP,
        };
        crate::template::transform_and_record_template(
            &mut template,
            &mut TransformSfcContext::anonymous(),
        );
        let [Node::ConditionalSeq(seq)] = template.roots.as_slice() else {
            panic!("v-if should form a conditional");
        };
        assert_eq!(to_str(&seq.if_node.condition), "_ctx.ok");
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L224-L232
    #[test]
    fn v_if_key_shorthand() {
        // <div v-if="ok" :key></div>
        let mut node = if_branch("div", "ok");
        let Node::Element(element) = &mut node else {
            panic!("v-if should contain an element");
        };
        element
            .starting_tag
            .attributes
            .push(AttributeOrBinding::VBind(VBindDirective {
                argument: Some("key".into()),
                // Note: Fervid does not support shorthands inside AST,
                // those are expanded by the parser
                value: js("key"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            }));
        let seq = fold_if(vec![node]);
        let Node::Element(element) = &seq.if_node.node else {
            panic!("v-if should retain its element");
        };
        let [AttributeOrBinding::VBind(key)] = element.starting_tag.attributes.as_slice() else {
            panic!("v-if should retain user shorthand key");
        };
        assert!(
            matches!(key.argument.as_ref(), Some(fervid_core::StrOrExpr::Str(arg)) if arg.value == "key")
        );
        assert_eq!(to_str(&key.value), "key");
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L235-L295
    #[test]
    fn error_on_v_else_missing_adjacent_v_if() {
        // <div v-else/>
        // <div/><div v-else/>
        // <div/>foo<div v-else/>
        // <div v-if="bar"/>foo<div v-else/>
        // <div v-if="bar"/>\u{00a0}<div v-else/>
        let cases = [
            vec![branch(
                "div",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            )],
            vec![
                Node::Element(element_from_tag("div")),
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                Node::Element(element_from_tag("div")),
                Node::Text("foo".into(), DUMMY_SP),
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                if_branch("div", "bar"),
                Node::Text("foo".into(), DUMMY_SP),
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                if_branch("div", "bar"),
                Node::Text("\u{00a0}".into(), DUMMY_SP),
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ],
        ];

        for mut children in cases {
            let mut ctx = TransformSfcContext::anonymous();
            transform_if(&mut ctx, &mut children, ElementKind::Element, 0);
            assert!(
                ctx.errors.iter().any(|error| matches!(
                    error,
                    TransformError::TemplateError(error)
                        if matches!(error.kind, TemplateErrorKind::VElseNoAdjacentIf)
                )),
                "Vue reports X_V_ELSE_NO_ADJACENT_IF: {:#?}",
                ctx.errors
            );
        }
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L297-L373
    #[test]
    fn error_on_v_else_if_missing_adjacent_v_if_or_v_else_if() {
        // <div v-else-if="foo"/>
        // <div/><div v-else-if="foo"/>
        // <div/>foo<div v-else-if="foo"/>
        // <div v-if="bar"/>foo<div v-else-if="foo"/>
        // <div v-if="bar"/>\u{00a0}<div v-else-if="foo"/>
        // <div v-if="notOk"/><div v-else/><div v-else-if="ok"/>
        let cases = [
            vec![branch(
                "div",
                VueDirectives {
                    v_else_if: Some(js("foo")),
                    ..Default::default()
                },
            )],
            vec![
                Node::Element(element_from_tag("div")),
                branch(
                    "div",
                    VueDirectives {
                        v_else_if: Some(js("foo")),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                Node::Element(element_from_tag("div")),
                Node::Text("foo".into(), DUMMY_SP),
                branch(
                    "div",
                    VueDirectives {
                        v_else_if: Some(js("foo")),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                if_branch("div", "bar"),
                Node::Text("foo".into(), DUMMY_SP),
                branch(
                    "div",
                    VueDirectives {
                        v_else_if: Some(js("foo")),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                if_branch("div", "bar"),
                Node::Text("\u{00a0}".into(), DUMMY_SP),
                branch(
                    "div",
                    VueDirectives {
                        v_else_if: Some(js("foo")),
                        ..Default::default()
                    },
                ),
            ],
            vec![
                if_branch("div", "notOk"),
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
                branch(
                    "div",
                    VueDirectives {
                        v_else_if: Some(js("ok")),
                        ..Default::default()
                    },
                ),
            ],
        ];

        for mut children in cases {
            let mut ctx = TransformSfcContext::anonymous();
            transform_if(&mut ctx, &mut children, ElementKind::Element, 0);
            assert!(
                ctx.errors.iter().any(|error| matches!(
                    error,
                    TransformError::TemplateError(error)
                        if matches!(error.kind, TemplateErrorKind::VElseNoAdjacentIf)
                )),
                "Vue reports X_V_ELSE_NO_ADJACENT_IF: {:#?}",
                ctx.errors
            );
        }
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L375-L392
    #[test]
    fn error_on_adjacent_v_else() {
        // <div v-if="false"/><div v-else/><div v-else/>
        let mut children = vec![
            if_branch("div", "false"),
            branch(
                "div",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
            branch(
                "div",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
        ];
        let mut ctx = TransformSfcContext::anonymous();

        transform_if(&mut ctx, &mut children, ElementKind::Element, 0);

        assert!(
            ctx.errors.iter().any(|error| matches!(
                error,
                TransformError::TemplateError(error)
                    if matches!(error.kind, TemplateErrorKind::VElseNoAdjacentIf)
            )),
            "Vue reports X_V_ELSE_NO_ADJACENT_IF: {:#?}",
            ctx.errors
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L394-L416
    #[test]
    fn error_on_user_key() {
        // <div v-if="ok" :key="a + 1" /><div v-else :key="a + 1" />
        // <div v-if="ok" key="1" /><div v-else key="1" />
        for attributes in [
            vec![AttributeOrBinding::VBind(VBindDirective {
                argument: Some("key".into()),
                value: js("a + 1"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            })],
            vec![AttributeOrBinding::RegularAttribute {
                name: "key".into(),
                value: Some("1".into()),
                span: DUMMY_SP,
            }],
        ] {
            let mut first = if_branch("div", "ok");
            let mut second = branch(
                "div",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            );
            let Node::Element(first_element) = &mut first else {
                panic!("v-if branch should contain an element");
            };
            first_element.starting_tag.attributes = attributes.clone();
            let Node::Element(second_element) = &mut second else {
                panic!("v-else branch should contain an element");
            };
            second_element.starting_tag.attributes = attributes;
            let mut ctx = TransformSfcContext::anonymous();

            transform_if(&mut ctx, &mut vec![first, second], ElementKind::Element, 0);

            assert_eq!(
                ctx.errors.len(),
                1,
                "Vue reports X_V_IF_SAME_KEY for duplicate user keys: {:#?}",
                ctx.errors
            );
            assert_eq!(same_key_errors(&ctx), 1, "{:#?}", ctx.errors);
        }
    }

    // Fervid-only: Vue currently misses duplicate rendered keys when a template carrier is discarded
    // Vue reference: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/src/transforms/vIf.ts#L207-L215
    #[test]
    fn child_key_overrides_ignored_template_key_and_collides() {
        // <template v-if="ok" key="outer"><div key="inner"/></template><div v-else key="inner"/>
        // What happens here - the first `<template>` will actually use the `inner` key
        // on the generated Fragment, not the `outer`. And `v-else` will also use its own `inner` key,
        // causing a clash during rendering. Fervid catches this edge case.
        let mut children = vec![
            template_branch(
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![regular_key("outer")],
                vec![with_attributes(element("div"), vec![regular_key("inner")])],
            ),
            with_attributes(
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
                vec![regular_key("inner")],
            ),
        ];
        let mut ctx = TransformSfcContext::anonymous();
        transform_template_attributes(&mut ctx, &children);
        transform_if(&mut ctx, &mut children, ElementKind::Element, 0);

        // `key="inner"` clash gets reported
        assert_eq!(same_key_errors(&ctx), 1, "{:#?}", ctx.errors);

        // `key="outer"` also does
        assert_eq!(
            discarded_template_attribute_errors(&ctx),
            1,
            "{:#?}",
            ctx.errors
        );

        assert_eq!(ctx.errors.len(), 2, "{:#?}", ctx.errors);
        let [Node::ConditionalSeq(seq)] = children.as_slice() else {
            panic!("v-if and v-else should form one sequence");
        };
        assert!(matches!(
                &seq.if_node.node,
                Node::Element(element) if element.starting_tag.tag_name == "div" && element.starting_tag.attributes.iter().any(|attr| matches!(attr, AttributeOrBinding::RegularAttribute { name, value: Some(value), .. } if name == "key" && value == "inner"))));
    }

    // Fervid-only: Vue reports a false duplicate for the ignored outer key
    // Vue reference: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/src/transforms/vIf.ts#L170-L185
    #[test]
    fn ignored_template_key_does_not_collide_with_other_branch() {
        // <template v-if="ok" key="outer"><div key="inner"/></template><div v-else key="outer"/>
        let mut children = vec![
            template_branch(
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![regular_key("outer")],
                vec![with_attributes(element("div"), vec![regular_key("inner")])],
            ),
            with_attributes(
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
                vec![regular_key("outer")],
            ),
        ];
        let mut ctx = TransformSfcContext::anonymous();
        transform_template_attributes(&mut ctx, &children);
        transform_if(&mut ctx, &mut children, ElementKind::Element, 0);

        // Fervid does not report `key="outer"` as clash...
        assert_eq!(same_key_errors(&ctx), 0, "{:#?}", ctx.errors);
        // but rather as a discarded attribute
        assert_eq!(
            discarded_template_attribute_errors(&ctx),
            1,
            "{:#?}",
            ctx.errors
        );
        assert_eq!(ctx.errors.len(), 1, "{:#?}", ctx.errors);
    }

    // Fervid-only: literal numeric key can equal a generated branch key
    // Vue issue: https://github.com/vuejs/core/issues/13881
    #[test]
    fn explicit_numeric_key_collides_with_generated_branch_key() {
        // <template v-if="ok"><div/></template><div v-else :key="0"/>
        // What happens here - the first template gets auto-generated key `0`,
        // but the `div v-else` specifies key `0` which gets detected as a clash
        assert_diagnostics(
            vec![
                template_branch(
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                    vec![],
                    vec![element("div")],
                ),
                with_attributes(
                    branch(
                        "div",
                        VueDirectives {
                            v_else: Some(()),
                            ..Default::default()
                        },
                    ),
                    vec![bound_key("0")],
                ),
            ],
            1,
            0,
        );
    }

    // Fervid-only: Vue issue #13881 also applies when the explicit number is on the first branch
    // https://github.com/vuejs/core/issues/13881
    #[test]
    fn explicit_if_key_collides_with_generated_else_key() {
        // <div v-if="ok" :key="1"/><div v-else/>
        // Same as the previous test, but now `div v-else` gets auto-generated key `1`
        assert_diagnostics(
            vec![
                with_attributes(if_branch("div", "ok"), vec![bound_key("1")]),
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
            ],
            1,
            0,
        );
    }

    // Fervid-only: generated numeric key 0 differs from static attribute string "0"
    // Vue reference: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/src/transforms/vIf.ts#L240-L254
    #[test]
    fn generated_numeric_key_does_not_collide_with_static_string_key() {
        // <template v-if="ok"><div/></template><div v-else key="0"/>
        // Auto-generated `0` (number) is not the same as `"0"` (string)
        assert_diagnostics(
            vec![
                template_branch(
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                    vec![],
                    vec![element("div")],
                ),
                with_attributes(
                    branch(
                        "div",
                        VueDirectives {
                            v_else: Some(()),
                            ..Default::default()
                        },
                    ),
                    vec![regular_key("0")],
                ),
            ],
            0,
            0,
        );
    }

    // Fervid-only: a later v-else must compare against the original v-if, not only the preceding branch
    #[test]
    fn nonadjacent_branches_with_same_rendered_key_collide() {
        // <div v-if="ok" key="same"/><p v-else-if="other" key="different"/><b v-else key="same"/>
        assert_diagnostics(
            vec![
                with_attributes(if_branch("div", "ok"), vec![regular_key("same")]),
                with_attributes(
                    branch(
                        "p",
                        VueDirectives {
                            v_else_if: Some(js("other")),
                            ..Default::default()
                        },
                    ),
                    vec![regular_key("different")],
                ),
                with_attributes(
                    branch(
                        "b",
                        VueDirectives {
                            v_else: Some(()),
                            ..Default::default()
                        },
                    ),
                    vec![regular_key("same")],
                ),
            ],
            1,
            0,
        );
    }

    // Fervid-only: static string and bound string literal produce the same runtime key
    #[test]
    fn static_and_bound_string_keys_collide() {
        // <div v-if="ok" key="same"/><div v-else :key="'same'"/>
        assert_diagnostics(static_and_bound_key_branches("same", "'same'"), 1, 0);
    }

    // Fervid-only: static attribute string "0" differs from numeric v-bind 0
    #[test]
    fn static_string_key_does_not_collide_with_bound_number() {
        // <div v-if="ok" key="0"/><div v-else :key="0"/>
        assert_diagnostics(static_and_bound_key_branches("0", "0"), 0, 0);
    }

    // Fervid-only: valueless key and explicitly empty key both render as the empty string
    #[test]
    fn bare_and_empty_branch_keys_collide() {
        // <div v-if="ok" key/><div v-else key=""/>
        assert_diagnostics(
            vec![
                with_attributes(
                    if_branch("div", "ok"),
                    vec![AttributeOrBinding::RegularAttribute {
                        name: "key".into(),
                        value: None,
                        span: DUMMY_SP,
                    }],
                ),
                with_attributes(
                    branch(
                        "div",
                        VueDirectives {
                            v_else: Some(()),
                            ..Default::default()
                        },
                    ),
                    vec![regular_key("")],
                ),
            ],
            1,
            0,
        );
    }

    // Fervid-only: comparison follows expression structure instead of Vue's source-text comparison
    // Vue comparator: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/src/transforms/vIf.ts#L305-L332
    #[test]
    fn differently_spaced_key_expressions_collide() {
        // <div v-if="ok" :key="a+1"/><div v-else :key="a + 1"/>
        assert_diagnostics(
            vec![
                with_attributes(if_branch("div", "ok"), vec![bound_key("a+1")]),
                with_attributes(
                    branch(
                        "div",
                        VueDirectives {
                            v_else: Some(()),
                            ..Default::default()
                        },
                    ),
                    vec![bound_key("a + 1")],
                ),
            ],
            1,
            0,
        );
    }

    // Fervid-only: arbitrary v-bind can overwrite an injected key.
    // We can't tell if there is a collision or not - we assume user knows
    // what they are doing (v-bind can inject any key).
    #[test]
    fn unknown_spread_key_does_not_report_unproven_collision() {
        // <div v-if="ok" v-bind="attrs"/><div v-else :key="0"/>
        let spread = AttributeOrBinding::VBind(VBindDirective {
            argument: None,
            value: js("attrs"),
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: DUMMY_SP,
        });
        assert_diagnostics(
            vec![
                with_attributes(if_branch("div", "ok"), vec![spread]),
                with_attributes(
                    branch(
                        "div",
                        VueDirectives {
                            v_else: Some(()),
                            ..Default::default()
                        },
                    ),
                    vec![bound_key("0")],
                ),
            ],
            0,
            0,
        );
    }

    // Vue test: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L637-L660
    #[test]
    fn sibling_v_if_sequences_have_distinct_key_bases() {
        // <div v-if="ok"/><p v-if="orNot"/>
        let mut children = vec![if_branch("div", "ok"), if_branch("p", "orNot")];

        transform_if(
            &mut TransformSfcContext::anonymous(),
            &mut children,
            ElementKind::Element,
            0,
        );

        let [Node::ConditionalSeq(first), Node::ConditionalSeq(second)] = children.as_slice()
        else {
            panic!("independent v-if siblings should become separate sequences");
        };
        assert_eq!(first.key_base, 0);
        assert_eq!(second.key_base, 1);
    }

    // Additional plain sibling variant of Vue's increasing-key test
    // Vue test: https://github.com/vuejs/core/blob/bf5f0dd877dd3c23ce85c2cbc1dd771567e84c8c/packages/compiler-core/__tests__/transforms/vIf.spec.ts#L662-L700
    #[test]
    fn key_base_counts_preceding_branches_not_plain_siblings() {
        // <div v-if="ok"/><p v-else/><hr/><div v-if="another"/><p v-else-if="orNot"/><p v-else/>
        let mut children = vec![
            if_branch("div", "ok"),
            branch(
                "p",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
            Node::Element(element_from_tag("hr")),
            if_branch("div", "another"),
            branch(
                "p",
                VueDirectives {
                    v_else_if: Some(js("orNot")),
                    ..Default::default()
                },
            ),
            branch(
                "p",
                VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                },
            ),
        ];

        transform_if(
            &mut TransformSfcContext::anonymous(),
            &mut children,
            ElementKind::Element,
            0,
        );

        let [
            Node::ConditionalSeq(first),
            Node::Element(_),
            Node::ConditionalSeq(second),
        ] = children.as_slice()
        else {
            panic!("plain sibling should separate conditional sequences");
        };
        assert_eq!(first.key_base, 0);
        assert!(first.else_node.is_some());
        assert_eq!(second.key_base, 2);
        assert_eq!(second.else_if_nodes.len(), 1);
        assert!(second.else_node.is_some());
    }

    fn assert_diagnostics(
        mut children: Vec<Node>,
        expected_errors: usize,
        expected_discarded_attributes: usize,
    ) {
        assert!(
            expected_errors >= expected_discarded_attributes,
            "discarded-attribute errors must be part of total error count"
        );
        let mut ctx = TransformSfcContext::anonymous();
        transform_template_attributes(&mut ctx, &children);
        transform_if(&mut ctx, &mut children, ElementKind::Element, 0);
        assert_eq!(ctx.errors.len(), expected_errors, "{:#?}", ctx.errors);
        assert_eq!(
            discarded_template_attribute_errors(&ctx),
            expected_discarded_attributes,
            "{:#?}",
            ctx.errors
        );
        assert_eq!(
            same_key_errors(&ctx),
            expected_errors - expected_discarded_attributes,
            "{:#?}",
            ctx.errors
        );
    }

    fn same_key_errors(ctx: &TransformSfcContext) -> usize {
        ctx.errors
            .iter()
            .filter(|error| {
                matches!(
                    error,
                    TransformError::TemplateError(error)
                        if matches!(error.kind, TemplateErrorKind::VIfSameKey)
                )
            })
            .count()
    }

    fn discarded_template_attribute_errors(ctx: &TransformSfcContext) -> usize {
        ctx.errors
            .iter()
            .filter(|error| {
                matches!(
                    error,
                    TransformError::TemplateError(error)
                        if matches!(error.kind, TemplateErrorKind::TemplateDiscardedAttribute)
                )
            })
            .count()
    }

    fn element(tag: &str) -> Node {
        Node::Element(element_from_tag(tag))
    }

    fn with_attributes(mut node: Node, attributes: Vec<AttributeOrBinding>) -> Node {
        let Node::Element(element) = &mut node else {
            panic!("keyed fixture must contain an element");
        };
        element.starting_tag.attributes = attributes;
        node
    }

    fn template_branch(
        directives: VueDirectives,
        attributes: Vec<AttributeOrBinding>,
        children: Vec<Node>,
    ) -> Node {
        let mut element = element_from_tag("template");
        element.tag_type = ElementKind::Template;
        element.starting_tag.directives = Some(Box::new(directives));
        element.starting_tag.attributes = attributes;
        element.children = children;
        Node::Element(element)
    }

    fn regular_key(value: &str) -> AttributeOrBinding {
        regular_attribute("key", value)
    }

    fn regular_attribute(name: &str, value: &str) -> AttributeOrBinding {
        AttributeOrBinding::RegularAttribute {
            name: name.into(),
            value: Some(value.into()),
            span: DUMMY_SP,
        }
    }

    fn bound_key(value: &str) -> AttributeOrBinding {
        AttributeOrBinding::VBind(VBindDirective {
            argument: Some("key".into()),
            value: js(value),
            is_camel: false,
            is_prop: false,
            is_attr: false,
            span: DUMMY_SP,
        })
    }

    fn static_and_bound_key_branches(static_key: &str, expression: &str) -> Vec<Node> {
        vec![
            with_attributes(if_branch("div", "ok"), vec![regular_key(static_key)]),
            with_attributes(
                branch(
                    "div",
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                ),
                vec![bound_key(expression)],
            ),
        ]
    }

    fn fold_if(mut children: Vec<Node>) -> ConditionalNodeSequence {
        transform_if(
            &mut TransformSfcContext::anonymous(),
            &mut children,
            ElementKind::Element,
            0,
        );
        assert_eq!(
            1,
            children.len(),
            "input should fold into one conditional sequence"
        );
        let Some(Node::ConditionalSeq(seq)) = children.pop() else {
            panic!("Expected ConditionalSeq to be the child");
        };
        seq
    }

    fn branch(tag: &str, directives: VueDirectives) -> Node {
        let mut element = element_from_tag(tag);
        element.starting_tag.directives = Some(Box::new(directives));
        Node::Element(element)
    }

    fn if_branch(tag: &str, condition: &str) -> Node {
        branch(
            tag,
            VueDirectives {
                v_if: Some(js(condition)),
                ..Default::default()
            },
        )
    }
}
