#[cfg(not(feature = "new-pipeline"))]
use fervid_core::{
    AttributeOrBinding, BindingTypes, BuiltinType, FervidAtom, IntoIdent, TemplateGenerationMode,
    VBindDirective,
};
use fervid_core::{
    ConditionalNodeSequence, ElementKind, ElementNode, ForNode, Interpolation, Node, PatchFlags,
    PatchHints, SfcTemplateBlock, StartingTag, fervid_atom,
};
#[cfg(not(feature = "new-pipeline"))]
use fervid_core::{StrOrExpr, VSlotDirective, check_attribute_name};
use smallvec::SmallVec;
#[cfg(not(feature = "new-pipeline"))]
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Bool, Expr, Lit},
};

use crate::{
    TemplateScope, TransformSfcContext,
    template::{
        core::scope_tracking::{enter_element_scope, restore_element_scope_snapshot},
        node_transforms::NodeTransforms,
    },
};

#[cfg(not(feature = "new-pipeline"))]
use super::asset_urls::transform_asset_urls;
#[cfg(not(feature = "new-pipeline"))]
use super::collect_vars::collect_variables;
use super::expr_transform::BindingsHelperTransform;

pub struct TemplateVisitor<'s> {
    pub ctx: &'s mut TransformSfcContext,
    pub current_scope: u32,
    pub v_for_scope: bool,
}

/// Transforms the AST template by using information from [`BindingsHelper`].
///
/// The transformations tackled:
/// - Optimizing the tree by removing white-space nodes;
/// - Folding the conditional nodes (`v-if`, etc.) into a single `ConditionalNode`;
/// - Transforming Js expressions by resolving variables inside them.
pub fn transform_and_record_template(
    template: &mut SfcTemplateBlock,
    ctx: &mut TransformSfcContext,
) {
    #[cfg(feature = "new-pipeline")]
    if ctx.bindings_helper.template_scopes.is_empty() {
        ctx.bindings_helper.template_scopes.push(TemplateScope {
            variables: SmallVec::new(),
            parent: 0,
        });
    }

    // Optimize conditional sequences within template root
    let node_transforms = ctx.node_transforms.clone();
    node_transforms.pre_transform_children(ctx, &mut template.roots, ElementKind::Element);

    // Merge more than 1 child into a separate `<template>` element so that Fragment gets generated.
    // #11: Do this only when not all children are `TextNode`s.
    if template.roots.len() > 1
        && !template
            .roots
            .iter()
            .all(|r| matches!(r, Node::Text(_, _) | Node::Interpolation(_)))
    {
        let all_roots = std::mem::replace(&mut template.roots, Vec::with_capacity(1));

        let mut patch_hints = PatchHints::default();
        patch_hints.flags |= PatchFlags::StableFragment;

        let dev = !ctx.bindings_helper.is_prod;
        if dev
            && all_roots
                .iter()
                .filter(|root| !matches!(root, Node::Comment(_, _)))
                .count()
                == 1
        {
            patch_hints.flags |= PatchFlags::DevRootFragment;
        }

        let new_root = Node::Element(ElementNode {
            tag_type: ElementKind::Element,
            starting_tag: StartingTag {
                tag_name: fervid_atom!("template"),
                attributes: vec![],
                directives: None,
            },
            children: all_roots,
            template_scope: 0,
            patch_hints,
            span: template.span,
            codegen_node: None,
        });
        template.roots.push(new_root);
    }

    let mut template_visitor = TemplateVisitor::new(ctx);

    for node in template.roots.iter_mut() {
        node.visit_mut_with(&mut template_visitor);
    }
}

trait Visitor {
    fn visit_node(&mut self, node: &mut Node);
    fn visit_element_node(&mut self, element_node: &mut ElementNode);
    fn visit_for_node(&mut self, for_node: &mut ForNode);
    fn visit_conditional_node(&mut self, conditional_node: &mut ConditionalNodeSequence);
    #[cfg_attr(feature = "new-pipeline", allow(dead_code))]
    fn visit_interpolation(&mut self, interpolation: &mut Interpolation);
}

trait VisitMut {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor);
}

impl Visitor for TemplateVisitor<'_> {
    fn visit_node(&mut self, node: &mut Node) {
        #[cfg(feature = "new-pipeline")]
        {
            let node_transforms = self.ctx.node_transforms.clone();
            node_transforms.pre_transform_node(self.ctx, node);

            match node {
                Node::Element(element) => self.visit_element_node(element),
                Node::For(for_node) => self.visit_for_node(for_node),
                Node::ConditionalSeq(conditional) => self.visit_conditional_node(conditional),
                Node::Interpolation(_interpolation) => {
                    // self.visit_interpolation(interpolation)
                }
                Node::Text(_, _) | Node::Comment(_, _) => {}
            }

            node_transforms.post_transform_node(self.ctx, node);
        }

        #[cfg(not(feature = "new-pipeline"))]
        match node {
            Node::Element(el) => self.visit_element_node(el),
            Node::For(for_node) => self.visit_for_node(for_node),
            Node::ConditionalSeq(cond) => self.visit_conditional_node(cond),
            Node::Interpolation(interpolation) => self.visit_interpolation(interpolation),
            _ => {}
        }
    }

    fn visit_element_node(&mut self, element_node: &mut ElementNode) {
        #[cfg(feature = "new-pipeline")]
        return self.visit_element_node_new(element_node);

        #[cfg(not(feature = "new-pipeline"))]
        return self.visit_element_node_old(element_node);
    }

    fn visit_conditional_node(&mut self, conditional_node: &mut ConditionalNodeSequence) {
        // In this function, conditions are transformed first
        // without updating the template scope and collecting its variables.
        // I believe this is a correct way of doing it, because in VDOM the condition
        // wraps around the node (`condition ? if_node : else_node`).
        // However, I am not too sure about the `v-if` & `v-slot` combined usage.

        self.ctx
            .bindings_helper
            .transform_expr(&mut conditional_node.if_node.condition, self.current_scope);
        conditional_node.if_node.node.visit_mut_with(self);

        for else_if_node in conditional_node.else_if_nodes.iter_mut() {
            self.ctx
                .bindings_helper
                .transform_expr(&mut else_if_node.condition, self.current_scope);
            else_if_node.node.visit_mut_with(self);
        }

        if let Some(ref mut else_node) = conditional_node.else_node {
            else_node.visit_mut_with(self);
        }
    }

    fn visit_for_node(&mut self, for_node: &mut ForNode) {
        // TODO: Refactor this to re-use existing scope logic
        let old_scope = self.current_scope;
        let old_ctx_scope = self.ctx.current_template_scope;
        let old_v_for_scope = self.v_for_scope;

        self.current_scope = for_node.template_scope;
        self.ctx.current_template_scope = for_node.template_scope;

        // TODO: Remove this after all code is migrated to directive_scopes
        self.v_for_scope = true;

        let node_transforms = self.ctx.node_transforms.clone();
        node_transforms.pre_transform_children(
            self.ctx,
            &mut for_node.children,
            ElementKind::Template,
        );

        for child in for_node.children.iter_mut() {
            child.visit_mut_with(self);
        }

        self.current_scope = old_scope;
        self.ctx.current_template_scope = old_ctx_scope;
        self.v_for_scope = old_v_for_scope;
    }

    fn visit_interpolation(&mut self, interpolation: &mut Interpolation) {
        interpolation.template_scope = self.current_scope;

        let has_js = self
            .ctx
            .bindings_helper
            .transform_expr(&mut interpolation.value, self.current_scope);

        interpolation.patch_flag = has_js;
    }
}

impl TemplateVisitor<'_> {
    pub fn new(ctx: &'_ mut TransformSfcContext) -> TemplateVisitor<'_> {
        TemplateVisitor {
            ctx,
            current_scope: 0,
            v_for_scope: false,
        }
    }

    #[allow(unused)]
    fn visit_element_node_new(&mut self, element_node: &mut ElementNode) {
        // TODO: enter_element_scope does more than it should,
        // move transform_for and track_slot_scopes out of it

        // `v-for` has special behavior with `ref`
        let scope_snapshot = enter_element_scope(
            self.ctx,
            element_node,
            &mut self.current_scope,
            &mut self.v_for_scope,
        );

        // Cloning transforms is fine here due to the structure being optimized for it
        let node_transforms = self.ctx.node_transforms.clone();
        node_transforms.pre_transform_children(
            self.ctx,
            &mut element_node.children,
            element_node.tag_type,
        );

        for child in element_node.children.iter_mut() {
            child.visit_mut_with(self);
        }

        restore_element_scope_snapshot(
            self.ctx,
            scope_snapshot,
            &mut self.current_scope,
            &mut self.v_for_scope,
        );
    }

    #[cfg(not(feature = "new-pipeline"))]
    fn visit_element_node_old(&mut self, element_node: &mut ElementNode) {
        let parent_scope = self.current_scope;
        let mut scope_to_use = parent_scope;

        // Mark the node with a correct type (element, component or built-in)
        let element_kind = element_node.tag_type;
        let is_component = matches!(element_kind, ElementKind::Component);

        if is_component {
            self.maybe_resolve_component(&element_node.starting_tag.tag_name);
        }

        // `v-for` has special behavior with `ref`
        let old_v_for_scope = self.v_for_scope;

        // Patch hints
        // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L406
        let has_children = !element_node.children.is_empty();
        let mut has_dynamic_keys = false;
        let mut has_hydration_event_binding = false;
        let mut has_ref = false;
        let mut has_runtime_directives = false;
        let mut has_vnode_hook = false;
        let mut ref_key = Option::<FervidAtom>::None;
        let mut should_use_block = false;

        // Check if there is a scoping directive.
        // Find a `v-for` or `v-slot` directive when in ElementNode
        // and collect their variables into the new template scope
        if let Some(ref mut directives) = element_node.starting_tag.directives {
            let v_for = directives.v_for.as_mut();
            let v_slot = directives.v_slot.as_mut();

            // Create a new scope
            if v_for.is_some() || v_slot.is_some() {
                // New scope will have ID equal to length
                scope_to_use = self.ctx.bindings_helper.template_scopes.len() as u32;
                self.ctx
                    .bindings_helper
                    .template_scopes
                    .push(TemplateScope {
                        variables: SmallVec::new(),
                        parent: parent_scope,
                    });
            }

            // Collect `v-for` bindings
            if let Some(v_for) = v_for {
                self.v_for_scope = true;

                // Get the iterator variables and collect their variables
                let scope = &mut self.ctx.bindings_helper.template_scopes[scope_to_use as usize];
                collect_variables(&v_for.parse_result.value, scope);
                if let Some(key) = &v_for.parse_result.key {
                    collect_variables(key, scope);
                }
                if let Some(index) = &v_for.parse_result.index {
                    collect_variables(index, scope);
                }

                // Transform the source expression
                let is_dynamic = self
                    .ctx
                    .bindings_helper
                    .transform_expr(&mut v_for.parse_result.source, scope_to_use);

                // Add patch flags
                if !is_dynamic {
                    // This is `64 /* STABLE_FRAGMENT */`
                    // when iterable is non-dynamic (number, string) (`v-for="i in 3"`)
                    v_for.patch_flags |= PatchFlags::StableFragment;
                } else {
                    // Look for `key`. Fragment is either keyed or unkeyed.
                    let has_key = element_node
                        .starting_tag
                        .attributes
                        .iter()
                        .any(|attr| check_attribute_name(attr, "key"));

                    v_for.patch_flags |= if has_key {
                        PatchFlags::KeyedFragment
                    } else {
                        PatchFlags::UnkeyedFragment
                    };
                }
            }

            // Collect `v-slot` bindings
            if let Some(VSlotDirective {
                slot_name, value, ..
            }) = v_slot
            {
                if let Some(v_slot_value) = value {
                    let scope =
                        &mut self.ctx.bindings_helper.template_scopes[scope_to_use as usize];
                    collect_variables(v_slot_value, scope);
                }

                // Transform `v-slot` argument if it is dynamic
                if let Some(StrOrExpr::Expr(expr)) = slot_name {
                    self.ctx.bindings_helper.transform_expr(expr, scope_to_use);
                }
            }
        }

        // Update the element's scope and the Visitor's current scope
        element_node.template_scope = scope_to_use;
        self.current_scope = scope_to_use;

        // TODO Refactor the directives transformation logic
        // and maybe the Visitor as well

        // Transform the VBind and VOn attributes, apply asset URLs transform
        for attr in element_node.starting_tag.attributes.iter_mut() {
            let patch_hints = &mut element_node.patch_hints;
            match attr {
                // The logic for the patch flags:
                // 1. Check if the attribute name is dynamic (`:foo` vs `:[foo]`) or ;
                //    If it is, clear the previous prop hints and set FULL_PROPS, then continue loop;
                // 2. Check if there is a Js variable in the value;
                //    If there is, check if it is a component
                // 2. Check if
                AttributeOrBinding::VBind(v_bind) => {
                    let has_bindings = self
                        .ctx
                        .bindings_helper
                        .transform_expr(&mut v_bind.value, scope_to_use);

                    // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L676
                    // Force hydration for v-bind with .prop modifier
                    if v_bind.is_prop {
                        patch_hints.flags |= PatchFlags::NeedHydration;
                    }

                    let Some(StrOrExpr::Str(ref argument)) = v_bind.argument else {
                        if let Some(StrOrExpr::Expr(expr)) = v_bind.argument.as_mut() {
                            self.ctx.bindings_helper.transform_expr(expr, scope_to_use);
                        }

                        // This is dynamic
                        // From docs: [FULL_PROPS is] exclusive with CLASS, STYLE and PROPS.
                        patch_hints.flags &=
                            !(PatchFlags::Props | PatchFlags::Class | PatchFlags::Style);
                        patch_hints.flags |= PatchFlags::FullProps;
                        patch_hints.props.clear();
                        has_dynamic_keys = true;
                        continue;
                    };

                    // Skip `key` prop
                    if argument == "key" {
                        // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L585
                        // #938: elements with dynamic keys should be forced into blocks
                        should_use_block = true;
                        continue;
                    }

                    // Skip `is` on `<component>`
                    if argument == "is"
                        && matches!(element_kind, ElementKind::Builtin(BuiltinType::Component))
                    {
                        continue;
                    }

                    // For `ref_for`
                    if self.v_for_scope && argument == "ref" {
                        has_ref = true;
                    }

                    // If we are FULL_PROPS already, do not add other props/class/style.
                    // Or if we do not need to add.
                    if !has_bindings || patch_hints.flags.contains(PatchFlags::FullProps) {
                        continue;
                    }

                    // Adding `class` and `style` bindings depends on `is_component`
                    // They are added to PROPS for the components.
                    if is_component {
                        patch_hints.flags |= PatchFlags::Props;
                        patch_hints.props.push(argument.to_owned());
                        continue;
                    }

                    if argument == "class" {
                        patch_hints.flags |= PatchFlags::Class;
                    } else if argument == "style" {
                        patch_hints.flags |= PatchFlags::Style;
                    } else {
                        patch_hints.flags |= PatchFlags::Props;
                        patch_hints.props.push(argument.to_owned());
                    }
                }

                AttributeOrBinding::VOn(v_on) => {
                    // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L589C54-L589C71
                    // inline before-update hooks need to force block so that it is invoked
                    // before children
                    if has_children
                        && matches!(&v_on.event, Some(StrOrExpr::Str(s)) if s == "vue:before-update")
                    {
                        should_use_block = true;
                    }

                    self.transform_v_on(v_on, scope_to_use);

                    // TODO Transform the event name beforehand (?) and make sure the condition is 100% the same
                    // https://github.com/vuejs/core/blob/f1068fc60ca511f68ff0aaedcc18b39124791d29/packages/compiler-core/src/transforms/transformElement.ts#L430
                    if let Some(StrOrExpr::Str(evt_name)) = v_on.event.as_ref() {
                        let has_v_node = evt_name.starts_with("vue:");

                        // TODO Adjust condition due to the latest transformation changes
                        if (!is_component
                            || matches!(element_kind, ElementKind::Builtin(BuiltinType::Component)))
                            && evt_name != "click"
                            && evt_name != "update:modelValue"
                            && evt_name != "update:model-value"
                            && !has_v_node
                        {
                            has_hydration_event_binding = true;
                        }

                        has_vnode_hook |= has_v_node;
                    } else {
                        // https://github.com/vuejs/core/blob/f1068fc60ca511f68ff0aaedcc18b39124791d29/packages/compiler-core/src/transforms/transformElement.ts#L605
                        has_dynamic_keys = true;
                    }
                }

                // Transform the regular `ref` in `inline` mode
                AttributeOrBinding::RegularAttribute { name, value, span } if name == "ref" => {
                    has_ref = true;

                    // Get the binding type regardless of template generation mode to mark the ref as "used".
                    // This is the importUsageCheck behavior of the official compiler
                    let binding_type = if value.is_empty() {
                        BindingTypes::Unresolved
                    } else {
                        self.ctx
                            .bindings_helper
                            .get_var_binding_type(scope_to_use, value)
                    };

                    // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L506
                    // In inline mode there is no setupState object, so we can't use string
                    // keys to set the ref. Instead, we need to transform it to pass the
                    // actual ref.
                    if !value.is_empty()
                        && matches!(
                            self.ctx.bindings_helper.template_generation_mode,
                            TemplateGenerationMode::Inline
                        )
                        && matches!(
                            binding_type,
                            BindingTypes::SetupLet
                                | BindingTypes::SetupRef
                                | BindingTypes::SetupMaybeRef
                                | BindingTypes::Imported
                        )
                    {
                        let span = span.to_owned();
                        let value = value.to_owned();
                        ref_key = Some(value.to_owned());

                        let _ = std::mem::replace(
                            attr,
                            AttributeOrBinding::VBind(VBindDirective {
                                argument: Some(StrOrExpr::Str(fervid_atom!("ref"))),
                                value: Box::new(Expr::Ident(value.into_ident_spanned(span))),
                                is_camel: false,
                                is_prop: false,
                                is_attr: false,
                                span,
                            }),
                        );
                    }
                }

                _ => {}
            }
        }

        // Transform asset URLs (e.g. `src` in `<img src="">`) when the option is enabled (yes by default)
        transform_asset_urls(element_node, self.ctx);

        // Transform the directives
        let patch_hints = &mut element_node.patch_hints;
        if let Some(ref mut directives) = element_node.starting_tag.directives {
            macro_rules! maybe_transform {
                ($key: ident) => {
                    match directives.$key.as_mut() {
                        Some(expr) => self.ctx.bindings_helper.transform_expr(expr, scope_to_use),
                        None => false,
                    }
                };
            }
            maybe_transform!(v_html);
            maybe_transform!(v_memo);
            maybe_transform!(v_show);
            maybe_transform!(v_text);

            for v_model in directives.v_model.iter_mut() {
                self.ctx
                    .bindings_helper
                    .transform_v_model(v_model, scope_to_use, patch_hints);
            }

            // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L700
            // custom dirs may use beforeUpdate so they need to force blocks
            // to ensure before-update gets called before children update
            if !directives.custom.is_empty() {
                has_runtime_directives = true;

                if has_children {
                    should_use_block = true;
                }
            }

            // Transform custom directives
            for custom_directive in directives.custom.iter_mut() {
                use crate::template::resolutions::maybe_resolve_directive;

                if let Some(ref mut value) = custom_directive.value {
                    self.ctx.bindings_helper.transform_expr(value, scope_to_use);
                }
                if let Some(StrOrExpr::Expr(ref mut argument)) = custom_directive.argument {
                    self.ctx
                        .bindings_helper
                        .transform_expr(argument, scope_to_use);
                }

                // Try resolving it
                maybe_resolve_directive(self.ctx, &custom_directive.name, scope_to_use);
            }
        }

        // Merge conditional nodes and clean up whitespace
        let node_transforms = self.ctx.node_transforms.clone();
        node_transforms.pre_transform_children(self.ctx, &mut element_node.children, element_kind);

        // Patch flag for HTML elements which only contain interpolation and text,
        // e.g. `<p>{{ msg }}</p>`.
        // Does not apply to components or child-less elements
        let mut is_children_text_only =
            matches!(element_kind, ElementKind::Element) && !element_node.children.is_empty();
        let mut has_dynamic_interpolation = false;

        // Recursively visit children
        for child in element_node.children.iter_mut() {
            child.visit_mut_with(self);

            match child {
                // When Elements are present, TEXT patch flag does not apply
                Node::Element(_) | Node::For(_) | Node::ConditionalSeq(_) => {
                    is_children_text_only = false;
                }

                // TEXT patch flag only applies when there is an interpolation with a patch flag
                Node::Interpolation(interpolation) => {
                    has_dynamic_interpolation |= interpolation.patch_flag;
                }

                Node::Text(_, _) | Node::Comment(_, _) => {}
            }
        }

        // Add `ref_for` and `ref_key`
        if has_ref && self.v_for_scope {
            element_node
                .starting_tag
                .attributes
                .push(AttributeOrBinding::VBind(VBindDirective {
                    argument: Some(StrOrExpr::Str(fervid_atom!("ref_for"))),
                    value: Box::new(Expr::Lit(Lit::Bool(Bool {
                        span: DUMMY_SP,
                        value: true,
                    }))),
                    is_camel: false,
                    is_prop: false,
                    is_attr: false,
                    span: DUMMY_SP,
                }));
        }
        if let Some(ref_key) = ref_key {
            element_node
                .starting_tag
                .attributes
                .push(AttributeOrBinding::RegularAttribute {
                    name: fervid_atom!("ref_key"),
                    value: ref_key,
                    span: DUMMY_SP,
                });
        }
        self.v_for_scope = old_v_for_scope;

        // Apply other flags
        // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L732
        if !has_dynamic_keys && has_hydration_event_binding {
            patch_hints.flags |= PatchFlags::NeedHydration;
        }
        if !should_use_block
            && (patch_hints.flags.is_empty() || patch_hints.flags == PatchFlags::NeedHydration)
            && (has_ref || has_vnode_hook || has_runtime_directives)
        {
            patch_hints.flags |= PatchFlags::NeedPatch;
        }

        // Apply TEXT patch flag
        if is_children_text_only && has_dynamic_interpolation {
            patch_hints.flags |= PatchFlags::Text;
        }

        // Restore the parent scope
        self.current_scope = parent_scope;
    }
}

impl VisitMut for Node {
    fn visit_mut_with(&mut self, visitor: &mut impl Visitor) {
        visitor.visit_node(self);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "new-pipeline")]
    use fervid_core::{
        AttributeOrBinding, ElementNodeCodegenNode, ExpressionNode, JsChildNode, PropsExpression,
        StrOrExpr, VBindDirective, VCustomDirective, VOnDirective, VueImports,
    };
    use fervid_core::{
        Conditional, ElementKind, ForParseResult, Node, PatchHints, VForDirective, VueDirectives,
    };
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{js, to_str};

    use super::*;

    #[test]
    fn it_folds_basic_seq() {
        // <template><div>
        //   text
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h3 v-else>else</h3>
        // </div></template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![text_node(), if_node(), else_if_node(), else_node()],
                template_scope: 0,
                tag_type: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template roots: one div
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref div) = sfc_template.roots[0] else {
            panic!("Root is not an element")
        };

        // Text and conditional seq
        assert_eq!(2, div.children.len());
        check_text_node(&div.children[0]);
        let Node::ConditionalSeq(seq) = &div.children[1] else {
            panic!("Not a conditional sequence")
        };

        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);

        // <h2 v-else-if="foo">else-if</h3>
        assert_eq!(1, seq.else_if_nodes.len());
        check_else_if_node(&seq.else_if_nodes[0]);

        // <h3 v-else>else</h3>
        check_else_node(seq.else_node.as_deref());
    }

    #[test]
    fn it_folds_roots() {
        // <template>
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h3 v-else>else</h3>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![if_node(), else_if_node(), else_node()],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template roots: one conditional sequence
        assert_eq!(1, sfc_template.roots.len());
        let Node::ConditionalSeq(ref seq) = sfc_template.roots[0] else {
            panic!("Root is not a conditional sequence")
        };

        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);

        // <h2 v-else-if="foo">else-if</h3>
        assert_eq!(1, seq.else_if_nodes.len());
        check_else_if_node(&seq.else_if_nodes[0]);

        // <h3 v-else>else</h3>
        check_else_node(seq.else_node.as_deref());
    }

    #[test]
    fn it_folds_multiple_ifs() {
        // <template>
        //   <h1 v-if="true">if</h1>
        //   <h1 v-if="true">if</h1>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![if_node(), if_node()],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template roots: two conditional sequences inside one root
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref root) = sfc_template.roots[0] else {
            panic!("root is not an element")
        };
        let Node::ConditionalSeq(ref seq) = root.children[0] else {
            panic!("root.children[0] is not a conditional sequence")
        };
        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);

        let Node::ConditionalSeq(ref seq) = root.children[1] else {
            panic!("root.children[1] not a conditional sequence")
        };
        // <h1 v-if="true">if</h1>
        check_if_node(&seq.if_node);
    }

    #[test]
    fn it_folds_multiple_else_ifs() {
        // <template>
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![if_node(), else_if_node(), if_node(), else_if_node()],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template roots: two conditional sequences inside one root
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref root) = sfc_template.roots[0] else {
            panic!("root is not an element")
        };
        let Node::ConditionalSeq(ref seq) = root.children[0] else {
            panic!("roots[0] is not a conditional sequence")
        };
        check_if_node(&seq.if_node);
        check_else_if_node(&seq.else_if_nodes[0]);

        let Node::ConditionalSeq(ref seq) = root.children[1] else {
            panic!("roots[1] not a conditional sequence")
        };
        check_if_node(&seq.if_node);
        check_else_if_node(&seq.else_if_nodes[0]);
    }

    #[test]
    fn it_leaves_bad_nodes() {
        // <template>
        //   <h2 v-else-if="foo">else-if</h2>
        //   <h3 v-else>else</h3>
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![else_if_node(), else_node()],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template root children: still two
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref root) = sfc_template.roots[0] else {
            panic!("root is not an element")
        };
        assert!(matches!(root.children[0], Node::Element(_)));
        assert!(matches!(root.children[1], Node::Element(_)));
    }

    #[test]
    fn it_merges_roots() {
        // #11: Should not get merged
        // <template>
        //   hello {{ 1 + 1 }}
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![
                Node::Text("text".into(), DUMMY_SP),
                Node::Interpolation(Interpolation {
                    value: js("1 + 1"),
                    template_scope: 0,
                    patch_flag: false,
                    span: DUMMY_SP,
                }),
            ],
            span: DUMMY_SP,
        };
        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());
        assert_eq!(2, sfc_template.roots.len());

        // Should get merged
        // <template>
        //   hello {{ 1 + 1 }}
        //   <div />
        // </template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![
                Node::Text("text".into(), DUMMY_SP),
                Node::Interpolation(Interpolation {
                    value: js("1 + 1"),
                    template_scope: 0,
                    patch_flag: false,
                    span: DUMMY_SP,
                }),
                Node::Element(ElementNode {
                    tag_type: ElementKind::Element,
                    starting_tag: StartingTag {
                        tag_name: "div".into(),
                        attributes: vec![],
                        directives: None,
                    },
                    children: vec![],
                    template_scope: 0,
                    patch_hints: PatchHints::default(),
                    span: DUMMY_SP,
                    codegen_node: None,
                }),
            ],
            span: DUMMY_SP,
        };
        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());
        assert_eq!(1, sfc_template.roots.len());
    }

    #[test]
    fn it_handles_complex_cases() {
        // <template><div>
        //   text
        //   <h1 v-if="true">if</h1>
        //   text
        //   <h1 v-if="true">if</h1>
        //   <h2 v-else-if="foo">else-if</h2>
        //   text
        //   <h1 v-if="true">if</h1>
        //   <h3 v-else>else</h3>
        // </div></template>
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div".into(),
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    text_node(),
                    if_node(),
                    text_node(),
                    if_node(),
                    else_if_node(),
                    text_node(),
                    if_node(),
                    else_node(),
                ],
                template_scope: 0,
                tag_type: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template roots: one div
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref div) = sfc_template.roots[0] else {
            panic!("Root is not an element")
        };

        // Text and conditional seq
        assert_eq!(6, div.children.len());
        check_text_node(&div.children[0]);
        check_text_node(&div.children[2]);
        check_text_node(&div.children[4]);
        assert!(matches!(&div.children[1], Node::ConditionalSeq(_)));
        assert!(matches!(&div.children[3], Node::ConditionalSeq(_)));
        assert!(matches!(&div.children[5], Node::ConditionalSeq(_)));
    }

    #[test]
    fn it_ignores_node_without_conditional_directives() {
        let no_directives1 = Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "test-component".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    ..Default::default()
                })),
            },
            children: vec![],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        });

        let no_directives2 = Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("hello".into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        });

        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![no_directives1, no_directives2],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        // Template root: both children nodes are still present
        assert_eq!(1, sfc_template.roots.len());
        let Node::Element(ref root) = sfc_template.roots[0] else {
            panic!("root is not an element")
        };
        assert_eq!(2, root.children.len());
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_tracks_v_for_aliases_on_template_slots() {
        let swc_core::ecma::ast::Expr::Ident(slot_prop) = *js("row") else {
            unreachable!()
        };

        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "template".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        v_for: Some(VForDirective {
                            parse_result: Box::new(ForParseResult {
                                source: js("item.items"),
                                value: js("item"),
                                key: Some(js("key")),
                                index: Some(js("index")),
                                finalized: false,
                                finalized_is_dynamic: false,
                            }),
                            patch_flags: Default::default(),
                            span: DUMMY_SP,
                        }),
                        v_slot: Some(fervid_core::VSlotDirective {
                            slot_name: None,
                            value: Some(Box::new(swc_core::ecma::ast::Pat::Ident(
                                slot_prop.into(),
                            ))),
                        }),
                        ..Default::default()
                    })),
                },
                children: vec![Node::Interpolation(Interpolation {
                    value: js("item + key + index + row + outside"),
                    template_scope: 0,
                    patch_flag: false,
                    span: DUMMY_SP,
                })],
                template_scope: 0,
                tag_type: ElementKind::Template,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        };
        let mut ctx = TransformSfcContext::anonymous();

        transform_and_record_template(&mut sfc_template, &mut ctx);

        let Node::Element(template) = &sfc_template.roots[0] else {
            panic!("Template v-for slot should remain an element")
        };
        let directives = template
            .starting_tag
            .directives
            .as_ref()
            .expect("template v-for slot should retain its directives");
        let v_for = directives
            .v_for
            .as_ref()
            .expect("template v-for slot should retain its v-for directive");
        assert!(v_for.parse_result.finalized);
        assert_eq!("_ctx.item.items", to_str(&v_for.parse_result.source));
        assert!(template.template_scope > 0);

        let [Node::Interpolation(interpolation)] = template.children.as_slice() else {
            panic!("Expected one interpolation")
        };
        assert_eq!(template.template_scope, interpolation.template_scope);
        assert_eq!(
            "item+key+index+row+_ctx.outside",
            to_str(&interpolation.value)
        );
        assert_eq!(0, ctx.directive_scopes.v_for);
        assert_eq!(0, ctx.directive_scopes.v_slot);
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_builds_props_for_directive_only_elements() {
        let mut sfc_template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: "div".into(),
                    attributes: vec![],
                    directives: Some(Box::new(VueDirectives {
                        custom: vec![fervid_core::VCustomDirective {
                            name: "focus".into(),
                            argument: None,
                            modifiers: vec![],
                            value: None,
                        }],
                        ..Default::default()
                    })),
                },
                children: vec![],
                template_scope: 0,
                tag_type: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        };

        transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

        let Node::Element(element) = &sfc_template.roots[0] else {
            panic!("Expected directive-only element")
        };
        let fervid_core::ElementNodeCodegenNode::VNodeCall(vnode_call) = element
            .codegen_node
            .as_deref()
            .expect("directive-only element should have a VNodeCall");
        assert_eq!(
            1,
            vnode_call
                .directives
                .as_ref()
                .expect("custom directive should produce runtime directive arguments")
                .elems
                .len()
        );
    }

    // https://github.com/vuejs/core/tree/d2c458be2542a628878cbfbdfebcb53b65d3e9f3/packages/compiler-dom/__tests__/transforms
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_transforms_v_html_and_removes_children() {
        let mut template = directive_element(
            VueDirectives {
                v_html: Some(js("html")),
                ..Default::default()
            },
            vec![Node::Text(fervid_atom!("ignored"), DUMMY_SP)],
        );
        let mut ctx = TransformSfcContext::anonymous();

        transform_and_record_template(&mut template, &mut ctx);

        let (element, vnode_call) = expect_element_vnode(&template.roots[0]);
        assert!(element.children.is_empty());
        assert!(vnode_call.directives.is_none());
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::Props));
        assert_eq!(vnode_call.patch_hints.props.as_slice(), ["innerHTML"]);
        let prop = expect_object_prop(vnode_call, "innerHTML");
        let JsChildNode::ExpressionNode(value) = &prop.value else {
            panic!("innerHTML should use the directive expression")
        };
        let ExpressionNode::SimpleExpression(value) = value.as_ref() else {
            panic!("v-html value should be a simple expression")
        };
        assert_eq!(to_str(value.ast.as_ref()), "_ctx.html");
        assert!(matches!(
            ctx.errors.as_slice(),
            [crate::error::TransformError::TemplateError(error)]
                if matches!(error.kind, crate::error::TemplateErrorKind::VHtmlWithChildren)
        ));
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_transforms_v_text_and_removes_children() {
        let mut template = directive_element(
            VueDirectives {
                v_text: Some(js("text")),
                ..Default::default()
            },
            vec![Node::Text(fervid_atom!("ignored"), DUMMY_SP)],
        );
        let mut ctx = TransformSfcContext::anonymous();

        transform_and_record_template(&mut template, &mut ctx);

        let (element, vnode_call) = expect_element_vnode(&template.roots[0]);
        assert!(element.children.is_empty());
        assert!(vnode_call.directives.is_none());
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::Props));
        assert_eq!(vnode_call.patch_hints.props.as_slice(), ["textContent"]);
        let prop = expect_object_prop(vnode_call, "textContent");
        let JsChildNode::CallExpression(value) = &prop.value else {
            panic!("dynamic v-text should call toDisplayString")
        };
        assert!(matches!(value.callee, VueImports::ToDisplayString));
        let [JsChildNode::ExpressionNode(argument)] = value.arguments.as_slice() else {
            panic!("toDisplayString should receive the directive expression")
        };
        let ExpressionNode::SimpleExpression(argument) = argument.as_ref() else {
            panic!("v-text value should be a simple expression")
        };
        assert_eq!(to_str(argument.ast.as_ref()), "_ctx.text");
        assert!(matches!(
            ctx.errors.as_slice(),
            [crate::error::TransformError::TemplateError(error)]
                if matches!(error.kind, crate::error::TemplateErrorKind::VTextWithChildren)
        ));
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_transforms_v_show_to_runtime_directive() {
        let mut template = directive_element(
            VueDirectives {
                v_show: Some(js("visible")),
                ..Default::default()
            },
            vec![Node::Text(fervid_atom!("content"), DUMMY_SP)],
        );
        let mut ctx = TransformSfcContext::anonymous();

        transform_and_record_template(&mut template, &mut ctx);

        let (element, vnode_call) = expect_element_vnode(&template.roots[0]);
        assert_eq!(element.children.len(), 1);
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::NeedPatch));
        assert_eq!(
            to_str(
                vnode_call
                    .directives
                    .as_ref()
                    .expect("v-show should produce runtime directive arguments")
            ),
            "[[_vShow,_ctx.visible]]"
        );
        assert!(ctx.bindings_helper.vue_imports.contains(VueImports::VShow));
        assert!(ctx.errors.is_empty());
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_erases_v_cloak_without_affecting_children() {
        let mut template = directive_element(
            VueDirectives {
                v_cloak: Some(()),
                ..Default::default()
            },
            vec![Node::Text(fervid_atom!("content"), DUMMY_SP)],
        );

        transform_and_record_template(&mut template, &mut TransformSfcContext::anonymous());

        let (element, vnode_call) = expect_element_vnode(&template.roots[0]);
        assert_eq!(element.children.len(), 1);
        assert!(vnode_call.props.is_none());
        assert!(vnode_call.directives.is_none());
        assert!(vnode_call.patch_hints.flags.is_empty());
        assert!(vnode_call.patch_hints.props.is_empty());
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn dynamic_v_on_uses_full_props_without_normalizing_handler_key() {
        let mut template = v_on_element(
            "div",
            ElementKind::Element,
            vec![VOnDirective {
                event: Some(StrOrExpr::Expr(js("event"))),
                handler: Some(js("handler")),
                modifiers: vec![],
                span: DUMMY_SP,
            }],
        );
        let mut ctx = TransformSfcContext::anonymous();

        transform_and_record_template(&mut template, &mut ctx);

        let (_, vnode_call) = expect_element_vnode(&template.roots[0]);
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::FullProps));
        let Some(PropsExpression::ObjectExpression(props)) = vnode_call.props.as_ref() else {
            panic!("Dynamic handler key should remain object props")
        };
        let [prop] = props.properties.as_slice() else {
            panic!("Expected one dynamic handler property")
        };
        assert!(prop.key.is_handler_key());
        let fervid_core::ExpressionPropNameNode::CompoundExpression(key) = &prop.key else {
            panic!("Dynamic handler should use a compound property key")
        };
        let swc_core::ecma::ast::PropName::Computed(key) = &key.ast else {
            panic!("Dynamic handler should use a computed property key")
        };
        assert_eq!(to_str(key.expr.as_ref()), "_toHandlerKey(_ctx.event)");
        assert!(
            !ctx.bindings_helper
                .vue_imports
                .contains(VueImports::NormalizeProps)
        );
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn vnode_hook_uses_need_patch() {
        let mut template = v_on_element(
            "div",
            ElementKind::Element,
            vec![VOnDirective {
                event: Some(StrOrExpr::Str(fervid_atom!("vue:mounted"))),
                handler: Some(js("handler")),
                modifiers: vec![],
                span: DUMMY_SP,
            }],
        );

        transform_and_record_template(&mut template, &mut TransformSfcContext::anonymous());

        let (_, vnode_call) = expect_element_vnode(&template.roots[0]);
        assert!(!vnode_call.needs_patch);
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::NeedPatch));
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn duplicate_static_v_on_handlers_are_deduped_into_array() {
        let mut template = v_on_element(
            "div",
            ElementKind::Element,
            vec![
                VOnDirective {
                    event: Some(StrOrExpr::Str(fervid_atom!("click"))),
                    handler: Some(js("first")),
                    modifiers: vec![],
                    span: DUMMY_SP,
                },
                VOnDirective {
                    event: Some(StrOrExpr::Str(fervid_atom!("click"))),
                    handler: Some(js("second")),
                    modifiers: vec![],
                    span: DUMMY_SP,
                },
            ],
        );

        transform_and_record_template(&mut template, &mut TransformSfcContext::anonymous());

        let (_, vnode_call) = expect_element_vnode(&template.roots[0]);
        let prop = expect_object_prop(vnode_call, "onClick");
        let JsChildNode::ArrayExpression(handlers) = &prop.value else {
            panic!("Duplicate handlers should produce an array")
        };
        let [_, _] = handlers.elements.as_slice() else {
            panic!("Expected both duplicate handlers")
        };
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn argumentless_v_on_marks_element_handlers_only() {
        for (tag_name, tag_type, expected_arg_count) in [
            ("div", ElementKind::Element, 2),
            ("Comp", ElementKind::Component, 1),
        ] {
            let mut template = v_on_element(
                tag_name,
                tag_type,
                vec![VOnDirective {
                    event: None,
                    handler: Some(js("listeners")),
                    modifiers: vec![],
                    span: DUMMY_SP,
                }],
            );

            transform_and_record_template(&mut template, &mut TransformSfcContext::anonymous());

            let (_, vnode_call) = expect_element_vnode(&template.roots[0]);
            let Some(PropsExpression::CallExpression(call)) = vnode_call.props.as_ref() else {
                panic!("Argumentless v-on should produce a helper call")
            };
            assert!(matches!(call.callee, VueImports::ToHandlers));
            assert_eq!(call.arguments.len(), expected_arg_count);
            let JsChildNode::ExpressionNode(listeners) = &call.arguments[0] else {
                panic!("toHandlers should receive listener expression")
            };
            dbg!(&listeners);
            let ExpressionNode::SimpleExpression(listeners) = listeners.as_ref() else {
                panic!("Listener should be a simple expression")
            };
            assert_eq!(to_str(listeners.ast.as_ref()), "_ctx.listeners");

            if expected_arg_count == 2 {
                let JsChildNode::ExpressionNode(is_element) = &call.arguments[1] else {
                    panic!("Element toHandlers should receive true")
                };
                let ExpressionNode::SimpleExpression(is_element) = is_element.as_ref() else {
                    panic!("Element marker should be a simple expression")
                };
                assert_eq!(to_str(is_element.ast.as_ref()), "true");
            }
        }
    }

    // https://github.com/vuejs/core/blob/02421cdbc4da5dd2eaf39e6c51aa790f9310db62/packages/compiler-core/__tests__/transforms/transformElement.spec.ts#L1037-L1134
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_preserves_lifecycle_requirements_in_stable_v_for() {
        let mut static_ref = for_element(
            js("3"),
            vec![
                AttributeOrBinding::VBind(VBindDirective {
                    argument: Some(StrOrExpr::Str(fervid_atom!("key"))),
                    value: js("i"),
                    is_camel: false,
                    is_prop: false,
                    is_attr: false,
                    span: DUMMY_SP,
                }),
                AttributeOrBinding::RegularAttribute {
                    name: fervid_atom!("ref"),
                    value: fervid_atom!("items"),
                    span: DUMMY_SP,
                },
            ],
            vec![],
            vec![],
        );
        transform_and_record_template(&mut static_ref, &mut TransformSfcContext::anonymous());

        let vnode_call = expect_for_vnode(&static_ref.roots[0]);
        assert!(!vnode_call.is_block);
        assert!(vnode_call.needs_patch);
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::NeedPatch));

        let mut childless_custom_directive = for_element(
            js("3"),
            vec![AttributeOrBinding::VBind(VBindDirective {
                argument: Some(StrOrExpr::Str(fervid_atom!("key"))),
                value: js("i"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            })],
            vec![VCustomDirective {
                name: fervid_atom!("dir"),
                ..Default::default()
            }],
            vec![],
        );
        transform_and_record_template(
            &mut childless_custom_directive,
            &mut TransformSfcContext::anonymous(),
        );

        let vnode_call = expect_for_vnode(&childless_custom_directive.roots[0]);
        assert!(!vnode_call.is_block);
        assert!(vnode_call.needs_patch);
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::NeedPatch));

        let mut custom_directive = for_element(
            js("3"),
            vec![],
            vec![VCustomDirective {
                name: fervid_atom!("dir"),
                ..Default::default()
            }],
            vec![Node::Interpolation(Interpolation {
                value: js("i"),
                template_scope: 0,
                patch_flag: false,
                span: DUMMY_SP,
            })],
        );
        transform_and_record_template(&mut custom_directive, &mut TransformSfcContext::anonymous());

        let vnode_call = expect_for_vnode(&custom_directive.roots[0]);
        assert!(vnode_call.is_block);
        assert!(vnode_call.is_block_required);
        assert!(vnode_call.patch_hints.flags.contains(PatchFlags::Text));
        assert!(!vnode_call.needs_patch);
        assert!(!vnode_call.patch_hints.flags.contains(PatchFlags::NeedPatch));
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_preserves_before_update_blocks_in_stable_v_for() {
        for event in ["vue:before-update", "vue:beforeUpdate"] {
            let mut template = for_element(
                js("3"),
                vec![AttributeOrBinding::VOn(VOnDirective {
                    event: Some(StrOrExpr::Str(event.into())),
                    handler: Some(js("handler")),
                    modifiers: vec![],
                    span: DUMMY_SP,
                })],
                vec![],
                vec![Node::Element(ElementNode::new(StartingTag {
                    tag_name: fervid_atom!("span"),
                    attributes: vec![],
                    directives: None,
                }))],
            );
            transform_and_record_template(&mut template, &mut TransformSfcContext::anonymous());

            let vnode_call = expect_for_vnode(&template.roots[0]);
            assert!(vnode_call.is_block, "{event} should preserve the block");
            assert!(
                vnode_call.is_block_required,
                "{event} should require the block"
            );
        }
    }

    #[cfg(feature = "new-pipeline")]
    #[test]
    fn it_keeps_dynamic_v_for_children_as_blocks() {
        let mut template = for_element(js("items"), vec![], vec![], vec![]);
        transform_and_record_template(&mut template, &mut TransformSfcContext::anonymous());

        let vnode_call = expect_for_vnode(&template.roots[0]);
        assert!(vnode_call.is_block);
        assert!(!vnode_call.is_block_required);
    }

    #[test]
    fn it_optimizes_nested_fragments() {
        // For cloning
        // <p>text</p>
        let p = ElementNode {
            tag_type: ElementKind::Element,
            starting_tag: StartingTag {
                tag_name: "p".into(),
                attributes: vec![],
                directives: Some(Default::default()),
            },
            children: vec![Node::Text("text".into(), DUMMY_SP)],
            template_scope: 0,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        };
        // <div v-if="false"></div>
        let div = ElementNode {
            tag_type: ElementKind::Element,
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_if: Some(js("false")),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("text".into(), DUMMY_SP)],
            template_scope: 0,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        };
        // <template></template>
        let tmpl = ElementNode {
            tag_type: ElementKind::Element,
            starting_tag: StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Default::default()),
            },
            children: vec![],
            template_scope: 0,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        };
        let sfc_tmpl = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![],
            span: DUMMY_SP,
        };

        // Convenience
        let prepare = |template_directives: Option<Box<VueDirectives>>,
                       p_directives: Option<Box<VueDirectives>>,
                       include_div: bool| {
            let mut p = p.clone();
            p.starting_tag.directives = p_directives;

            let mut template = tmpl.clone();
            if template_directives.is_some() {
                template.tag_type = ElementKind::Template;
            }
            template.starting_tag.directives = template_directives;
            template.children.push(Node::Element(p));

            let mut sfc_template = sfc_tmpl.clone();
            if include_div {
                sfc_template.roots.push(Node::Element(div.clone()));
            }
            sfc_template.roots.push(Node::Element(template));
            transform_and_record_template(&mut sfc_template, &mut TransformSfcContext::anonymous());

            let Some(Node::ConditionalSeq(cond)) = sfc_template.roots.pop() else {
                panic!("root is not a conditional seq")
            };

            cond
        };

        // Convenience
        macro_rules! directives {
            ($($directive:ident: $value:expr),* $(,)?) => {
                Box::new(VueDirectives {
                    $($directive: $value,)*
                    ..Default::default()
                })
            };
        }

        // <template v-if="val"><p>text</p></template>
        {
            let cond = prepare(Some(directives!(v_if: Some(js("val")))), None, false);

            // Folded to `<p v-if="val">text</p>`
            let cond_node = expect_element(&cond.if_node.node);
            assert!(cond_node.starting_tag.tag_name == "p");
            assert!(
                cond_node
                    .children
                    .first()
                    .is_some_and(|v| matches!(v, Node::Text(_, _)))
            )
        };

        // <template v-if="val" v-for="i in 3"><p>text</p></template>
        {
            let cond = prepare(
                Some(
                    directives!(v_if: Some(js("val")), v_for: Some(VForDirective {
                        parse_result: Box::new(ForParseResult {
                            source: js("3"),
                            value: js("i"),
                            key: None,
                            index: None,
                            finalized: false,
                            finalized_is_dynamic: false,
                        }),
                        patch_flags: Default::default(),
                        span: DUMMY_SP,
                    })),
                ),
                None,
                false,
            );

            // Folded to `<p v-if="val" v-for="i in 3">text</p>`
            #[cfg(feature = "new-pipeline")]
            let cond_node = expect_for_element(&cond.if_node.node);
            #[cfg(not(feature = "new-pipeline"))]
            let cond_node = expect_element(&cond.if_node.node);

            assert!(cond_node.starting_tag.tag_name == "p");
            assert!(
                cond_node
                    .children
                    .first()
                    .is_some_and(|v| matches!(v, Node::Text(_, _)))
            );
            let directives = cond_node.starting_tag.directives.as_ref();
            #[cfg(feature = "new-pipeline")]
            assert!(directives.is_some_and(|d| d.v_for.is_none()));
            #[cfg(not(feature = "new-pipeline"))]
            assert!(directives.is_some_and(|d| d.v_for.is_some()));
        };

        // <template v-if="val"><p v-for="j in 3">text</p></template>
        {
            let cond = prepare(
                Some(directives!(v_if: Some(js("val")))),
                Some(directives!(v_for: Some(VForDirective {
                    parse_result: Box::new(ForParseResult {
                        source: js("3"),
                        value: js("j"),
                        key: None,
                        index: None,
                        finalized: false,
                        finalized_is_dynamic: false,
                    }),
                    patch_flags: Default::default(),
                    span: DUMMY_SP,
                }))),
                false,
            );

            // Folded to `<p v-if="val" v-for="i in 3">text</p>`
            #[cfg(feature = "new-pipeline")]
            let cond_node = expect_for_element(&cond.if_node.node);
            #[cfg(not(feature = "new-pipeline"))]
            let cond_node = expect_element(&cond.if_node.node);
            assert!(cond_node.starting_tag.tag_name == "p");
            assert!(
                cond_node
                    .children
                    .first()
                    .is_some_and(|v| matches!(v, Node::Text(_, _)))
            );
            let directives = cond_node.starting_tag.directives.as_ref();
            #[cfg(feature = "new-pipeline")]
            assert!(directives.is_some_and(|d| d.v_for.is_none()));
            #[cfg(not(feature = "new-pipeline"))]
            assert!(directives.is_some_and(|d| d.v_for.is_some()));
        };

        // <template v-if="val" v-for="i in 3"><p v-for="j in 3">text</p></template>
        {
            let cond = prepare(
                Some(
                    directives!(v_if: Some(js("val")), v_for: Some(VForDirective {
                        parse_result: Box::new(ForParseResult {
                            source: js("3"),
                            value: js("i"),
                            key: None,
                            index: None,
                            finalized: false,
                            finalized_is_dynamic: false,
                        }),
                        patch_flags: Default::default(),
                        span: DUMMY_SP,
                    })),
                ),
                Some(directives!(v_for: Some(VForDirective {
                    parse_result: Box::new(ForParseResult {
                        source: js("3"),
                        value: js("j"),
                        key: None,
                        index: None,
                        finalized: false,
                        finalized_is_dynamic: false,
                    }),
                    patch_flags: Default::default(),
                    span: DUMMY_SP,
                }))),
                false,
            );

            // Not folded
            #[cfg(not(feature = "new-pipeline"))]
            {
                let cond_node = expect_element(&cond.if_node.node);
                assert!(cond_node.starting_tag.tag_name == "template");
                assert!(
                    cond_node
                        .starting_tag
                        .directives
                        .as_ref()
                        .is_some_and(|d| d.v_for.is_some())
                );

                let Some(Node::Element(first_child)) = cond_node.children.first() else {
                    panic!("First child should be an element")
                };
                assert!(first_child.starting_tag.tag_name == "p");
                assert!(
                    first_child
                        .starting_tag
                        .directives
                        .as_ref()
                        .is_some_and(|d| d.v_for.is_some())
                );
            }

            #[cfg(feature = "new-pipeline")]
            {
                let Node::For(outer_for) = &cond.if_node.node else {
                    panic!("Expected outer ForNode")
                };
                let [Node::For(inner_for)] = outer_for.children.as_slice() else {
                    panic!("Expected inner ForNode")
                };
                let [Node::Element(first_child)] = inner_for.children.as_slice() else {
                    panic!("Expected one Element child")
                };

                assert!(first_child.starting_tag.tag_name == "p");
                assert!(
                    first_child
                        .starting_tag
                        .directives
                        .as_ref()
                        .is_some_and(|d| d.v_for.is_none())
                );
            }
        };

        // <div v-if="false"></div>
        // <template v-else-if="val"><p>text</p></template>
        {
            let cond = prepare(Some(directives!(v_else_if: Some(js("val")))), None, true);

            // Folded to `<div v-if="false"></div><p v-else-if="val">text</p>`
            assert!(expect_element(&cond.if_node.node).starting_tag.tag_name == "div");
            let else_if_node =
                expect_element(&cond.else_if_nodes.first().expect("Should exist").node);
            assert!(else_if_node.starting_tag.tag_name == "p");
            assert!(
                else_if_node
                    .children
                    .first()
                    .is_some_and(|v| matches!(v, Node::Text(_, _)))
            );
        };

        // <div v-if="false"></div>
        // <template v-else><p>text</p></template>
        {
            let cond = prepare(Some(directives!(v_else: Some(()))), None, true);

            // Folded to `<div v-if="false"></div><p v-else-if="val">text</p>`
            assert!(expect_element(&cond.if_node.node).starting_tag.tag_name == "div");
            let else_node = cond.else_node.as_ref().expect("Should exist");
            let else_node = expect_element(else_node);
            assert!(else_node.starting_tag.tag_name == "p");
            assert!(
                else_node
                    .children
                    .first()
                    .is_some_and(|v| matches!(v, Node::Text(_, _)))
            );
        };
    }

    // text
    fn text_node() -> Node {
        Node::Text("text".into(), DUMMY_SP)
    }

    fn check_text_node(node: &Node) {
        assert!(matches!(node, Node::Text(text, span) if text == "text" && *span == DUMMY_SP));
    }

    fn expect_element(node: &Node) -> &ElementNode {
        let Node::Element(element) = node else {
            panic!("Expected element")
        };
        element
    }

    #[cfg(feature = "new-pipeline")]
    fn expect_for_element(node: &Node) -> &ElementNode {
        let Node::For(for_node) = node else {
            panic!("Expected ForNode")
        };
        let [Node::Element(element)] = for_node.children.as_slice() else {
            panic!("Expected one Element child")
        };
        element
    }

    #[cfg(feature = "new-pipeline")]
    fn expect_for_vnode(node: &Node) -> &fervid_core::VNodeCall {
        let element = expect_for_element(node);
        let Some(ElementNodeCodegenNode::VNodeCall(vnode_call)) = element.codegen_node.as_deref()
        else {
            panic!("Expected v-for child VNodeCall")
        };
        vnode_call
    }

    #[cfg(feature = "new-pipeline")]
    fn expect_element_vnode(node: &Node) -> (&ElementNode, &fervid_core::VNodeCall) {
        let element = expect_element(node);
        let Some(ElementNodeCodegenNode::VNodeCall(vnode_call)) = element.codegen_node.as_deref()
        else {
            panic!("Expected element VNodeCall")
        };
        (element, vnode_call)
    }

    #[cfg(feature = "new-pipeline")]
    fn expect_object_prop<'a>(
        vnode_call: &'a fervid_core::VNodeCall,
        expected_name: &str,
    ) -> &'a fervid_core::Property {
        let Some(PropsExpression::ObjectExpression(props)) = vnode_call.props.as_ref() else {
            panic!("Expected object props")
        };
        let [prop] = props.properties.as_slice() else {
            panic!("Expected one property")
        };
        let fervid_core::ExpressionPropNameNode::SimpleExpression(name) = &prop.key else {
            panic!("Expected static property name")
        };
        assert_eq!(name.ast.sym, expected_name);
        prop
    }

    #[cfg(feature = "new-pipeline")]
    fn directive_element(directives: VueDirectives, children: Vec<Node>) -> SfcTemplateBlock {
        SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: fervid_atom!("div"),
                    attributes: vec![],
                    directives: Some(Box::new(directives)),
                },
                children,
                template_scope: 0,
                tag_type: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        }
    }

    #[cfg(feature = "new-pipeline")]
    fn v_on_element(
        tag_name: &str,
        tag_type: ElementKind,
        directives: Vec<VOnDirective>,
    ) -> SfcTemplateBlock {
        let mut element = element_from_tag(tag_name);
        element.tag_type = tag_type;
        element.starting_tag.attributes = directives
            .into_iter()
            .map(AttributeOrBinding::VOn)
            .collect();

        SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(element)],
            span: DUMMY_SP,
        }
    }

    #[cfg(feature = "new-pipeline")]
    fn for_element(
        source: Box<swc_core::ecma::ast::Expr>,
        attributes: Vec<AttributeOrBinding>,
        custom_directives: Vec<VCustomDirective>,
        children: Vec<Node>,
    ) -> SfcTemplateBlock {
        SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name: fervid_atom!("div"),
                    attributes,
                    directives: Some(Box::new(VueDirectives {
                        v_for: Some(VForDirective {
                            parse_result: Box::new(ForParseResult {
                                source,
                                value: js("i"),
                                key: None,
                                index: None,
                                finalized: false,
                                finalized_is_dynamic: false,
                            }),
                            patch_flags: Default::default(),
                            span: DUMMY_SP,
                        }),
                        custom: custom_directives,
                        ..Default::default()
                    })),
                },
                children,
                template_scope: 0,
                tag_type: ElementKind::Element,
                patch_hints: Default::default(),
                span: DUMMY_SP,
                codegen_node: None,
            })],
            span: DUMMY_SP,
        }
    }

    // <h1 v-if="true">if</h1>
    fn if_node() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h1".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_if: Some(js("true")),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("if".into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })
    }

    fn check_if_node(if_node: &Conditional) {
        assert_eq!("true", to_str(&if_node.condition));
        assert!(matches!(
            &if_node.node,
            Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name,
                    ..
                },
                ..
            }) if tag_name == "h1"
        ));
    }

    // <h2 v-else-if="foo">else-if</h3>
    fn else_if_node() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h2".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_else_if: Some(js("foo")),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("else-if".into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })
    }

    fn check_else_if_node(else_if_node: &Conditional) {
        // condition, then node
        assert_eq!("_ctx.foo", to_str(&else_if_node.condition));
        assert!(matches!(
            &else_if_node.node,
            Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name,
                    ..
                },
                ..
            }) if tag_name == "h2"
        ));
    }

    // <h3 v-else>else</h3>
    fn else_node() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h3".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_else: Some(()),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("else".into(), DUMMY_SP)],
            template_scope: 0,
            tag_type: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
            codegen_node: None,
        })
    }

    fn check_else_node(else_node: Option<&Node>) {
        let else_node = else_node.expect("Must have else node");
        assert!(matches!(
            else_node,
            Node::Element(ElementNode {
                starting_tag: StartingTag {
                    tag_name,
                    ..
                },
                ..
            }) if tag_name == "h3"
        ));
    }
}
