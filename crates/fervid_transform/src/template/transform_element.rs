use std::borrow::Cow;

use fervid_core::{
    create_call_expression, create_object_expression, create_object_property,
    create_simple_expression_bool, create_simple_expression_propname, create_simple_expression_str,
    fervid_atom, AttributeOrBinding, BindingTypes, BuiltinType, CallExpression, ComponentBinding,
    ElementKind, ElementNode, ElementNodeCodegenNode, ExpressionNode, ExpressionPropNameNode,
    FervidAtom, IntoIdent, JsChildNode, Node, PatchFlags, PatchHints, Property, PropsExpression,
    SimpleExpressionNode, SimpleExpressionPropNameNode, StartingTag, StrOrExpr, VCustomDirective,
    VModelDirective, VNodeCall, VNodeCallTag, VNodeChildren, VueDirectives, VueImports,
};
use flagset::FlagSet;
use phf::phf_set;
use swc_core::{
    common::{util::take::Take, Span, Spanned, DUMMY_SP},
    ecma::ast::{ArrayLit, Expr, ExprOrSpread, IdentName, Lit, MemberExpr, MemberProp, Str},
};

use crate::{
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::{
        directive_transforms::DirectiveTransforms,
        expr_transform::BindingsHelperTransform,
        utils::{
            find_prop, is_core_component, is_static_arg_of, to_camel_case, to_pascal_case,
            to_valid_asset_id,
        },
    },
    BindingsHelper, SetupBinding, TransformSfcContext,
};

pub struct Props<'a> {
    pub attributes: &'a [AttributeOrBinding],
    pub directives: Option<&'a VueDirectives>,
}

pub enum RuntimeDirective {
    VShow(Box<Expr>),
    VModel(VModelDirective),
    Custom(VCustomDirective),
}

pub struct BuildPropsResult {
    // CallExpression | ObjectExpression | ExpressionNode
    pub props: Option<PropsExpression>,
    pub directives: Vec<RuntimeDirective>,
    // Patch `flags`, `dynamicPropNames` and `shouldUseBlock` are all inside PatchHints
    pub patch_hints: PatchHints,
}

#[derive(Default)]
pub struct PatchMarkers {
    pub dynamic_prop_names: Vec<FervidAtom>,
    pub flags: FlagSet<PatchFlags>,
    pub has_class_binding: bool,
    pub has_dynamic_keys: bool,
    pub has_hydration_event_binding: bool,
    pub has_ref: bool,
    pub has_runtime_directives: bool,
    pub has_style_binding: bool,
    pub has_vnode_hook: bool,
    pub should_use_block: bool,
}

pub fn post_transform_element_node(node: &mut ElementNode, ctx: &mut TransformSfcContext) {
    // TODO: This filter might discard built-in components which seem to be handled here
    // The official compiler only handles elements and components (no SLOT or TEMPLATE),
    // and implicitly considers built-in components here as well
    if !matches!(node.tag_type, ElementKind::Element | ElementKind::Component) {
        return;
    }

    let is_component = matches!(node.tag_type, ElementKind::Component);

    let vnode_tag = if is_component {
        resolve_component_type(node, ctx, false)
    } else {
        VNodeCallTag::Expr(Box::new(Expr::Lit(Lit::Str(Str::from(
            node.starting_tag.tag_name.to_owned(),
        )))))
    };

    let tag = node.starting_tag.tag_name.to_owned();
    let is_dynamic_component = matches!(vnode_tag, VNodeCallTag::CallExpression(_));

    let mut should_use_block = is_dynamic_component
        || matches!(&vnode_tag, VNodeCallTag::Builtin(BuiltinType::Teleport | BuiltinType::Suspense))
        // <svg> and <foreignObject> must be forced into blocks so that block
        // updates inside get proper isSVG flag at runtime. (#639, #643)
        // This is technically web-specific, but splitting the logic out of core
        // leads to too much unnecessary complexity.
        || (!is_component && matches!(tag.as_str(), "svg" | "foreignObject" | "math"));

    let mut vnode_props: Option<PropsExpression> = None;
    let mut vnode_directives: Option<ArrayLit> = None;
    let mut vnode_children: Option<VNodeChildren> = None;
    let mut patch_hints = PatchHints::default();

    // Props
    if !node.starting_tag.attributes.is_empty() {
        let props_build_result = build_props(
            node,
            ctx,
            ctx.current_template_scope,
            None,
            is_component,
            is_dynamic_component,
            false,
        );

        vnode_props = props_build_result.props;
        should_use_block |= props_build_result.patch_hints.should_use_block;
        patch_hints = props_build_result.patch_hints;

        if !props_build_result.directives.is_empty() {
            // Convert runtime directives into array-of-arrays
            let directives_array_elements = props_build_result
                .directives
                .into_iter()
                .map(|dir| {
                    Some(ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Array(build_directive_args(dir, ctx))),
                    })
                })
                .collect();
            vnode_directives = Some(ArrayLit {
                span: DUMMY_SP,
                elems: directives_array_elements,
            });
        }
    }

    // Children
    if !node.children.is_empty() {
        if matches!(vnode_tag, VNodeCallTag::Builtin(BuiltinType::KeepAlive)) {
            // Although a built-in component, we compile KeepAlive with raw children
            // instead of slot functions so that it can be used inside Transition
            // or other Transition-wrapping HOCs.
            // To ensure correct updates with block optimizations, we need to:
            // 1. Force keep-alive into a block. This avoids its children being
            //    collected by a parent block.
            should_use_block = true;
            // 2. Force keep-alive to always be updated, since it uses raw children.
            patch_hints.flags |= PatchFlags::DynamicSlots;
            if !ctx.bindings_helper.is_prod && node.children.len() > 1 {
                let span = Span {
                    lo: node
                        .children
                        .first()
                        .map(|child| child.span_lo())
                        .unwrap_or_else(|| node.span.lo),
                    hi: node
                        .children
                        .last()
                        .map(|child| child.span_hi())
                        .unwrap_or_else(|| node.span.hi),
                };
                ctx.errors
                    .push(TransformError::TemplateError(TemplateError {
                        span,
                        kind: TemplateErrorKind::KeepAliveInvalidChildren,
                    }));
            }
        }

        let should_build_as_slots = is_component
            && !matches!(
                vnode_tag,
                VNodeCallTag::Builtin(
                    // Teleport is not a real component and has dedicated runtime handling
                    BuiltinType::Teleport
                    // explained above.
                    | BuiltinType::KeepAlive
                )
            );

        if should_build_as_slots {
            todo!("implement buildSlots")
        } else if node.children.len() == 1
            && !matches!(vnode_tag, VNodeCallTag::Builtin(BuiltinType::Teleport))
        {
            let child = &node.children[0];
            // Note: Official implementation additionally checks for COMPOUND_EXPRESSION here
            // This is done because text + interpolation nodes are joined into a single COMPOUND_EXPRESSION node
            // https://github.com/vuejs/core/blob/623bfb29a23c36bf935e7faefa64d1baa69d465c/packages/compiler-core/src/transforms/transformElement.ts#L181-L201
            // TODO: Fervid doesn't yet merge Text nodes into a compound node
            // TODO: Consider constant type instead
            let mut has_dynamic_text_child = false;
            if let Node::Interpolation(interpolation) = child {
                has_dynamic_text_child = true;
                if interpolation.patch_flag {
                    patch_hints.flags |= PatchFlags::Text;
                }
            }
            if has_dynamic_text_child || matches!(child, Node::Text(_, _)) {
                vnode_children = Some(VNodeChildren::UseFirstChildTextNode);
            } else {
                vnode_children = Some(VNodeChildren::UseElementChildren);
            }
        } else {
            vnode_children = Some(VNodeChildren::UseElementChildren);
        }
    }

    // PatchFlag & dynamicPropNames
    // Note: stringification is not done here

    node.codegen_node = Some(Box::new(ElementNodeCodegenNode::VNodeCall(VNodeCall {
        tag: vnode_tag,
        props: vnode_props,
        children: vnode_children,
        patch_hints,
        directives: vnode_directives,
        is_block: should_use_block,
        disable_tracking: false,
        is_component,
    })));
}

/// https://github.com/vuejs/core/blob/aac7e1898907445c8f89b22047a9bfcf0a6e91b8/packages/compiler-core/src/transforms/transformElement.ts#L227-L320
fn resolve_component_type(
    node: &mut ElementNode,
    ctx: &mut TransformSfcContext,
    ssr: bool,
) -> VNodeCallTag {
    let mut tag = Cow::Borrowed(&node.starting_tag.tag_name);

    // 1. Dynamic component
    let is_explicit_dynamic = is_component_tag(&node.starting_tag);
    let is_prop = find_prop(node, "is", false, true);
    if let Some(is_prop) = is_prop {
        if is_explicit_dynamic {
            let mut exp: Option<ExpressionNode> = None;
            if let AttributeOrBinding::RegularAttribute { value, span, .. } = is_prop {
                exp = Some(ExpressionNode::SimpleExpression(
                    create_simple_expression_str(value.clone(), true, *span),
                ));
            } else if let AttributeOrBinding::VBind(v_bind_directive) = is_prop {
                // Note: we assume this is a simple expression while we don't have this information
                exp = Some(ExpressionNode::SimpleExpression(SimpleExpressionNode {
                    ast: v_bind_directive.value.clone(),
                    is_static: false,
                    const_type: fervid_core::ConstantTypes::NotConstant,
                    is_handler_key: false,
                }));
                // Note: the official transform handles `:is` shorthand expansion (`:is` -> `:is="is"`)
                // and transformation, but Fervid already expands the shorthands during parsing
            }

            if let Some(exp) = exp {
                return VNodeCallTag::CallExpression(create_call_expression(
                    ctx.bindings_helper
                        .helper(VueImports::ResolveDynamicComponent),
                    vec![JsChildNode::ExpressionNode(Box::new(exp))],
                    DUMMY_SP,
                ));
            }
        } else if let AttributeOrBinding::RegularAttribute { value, .. } = is_prop {
            if let Some(value_without_prefix) = value.strip_prefix("vue:") {
                // <button is="vue:xxx">
                // if not <component>, only is value that starts with "vue:" will be
                // treated as component by the parse phase and reach here, unless it's
                // compat mode where all is values are considered components
                tag = Cow::Owned(FervidAtom::from(value_without_prefix));
            }
        }
    }

    // 2. Built-in component (Teleport, Transition, KeepAlive, Suspense...)
    if let Some(built_in) = is_core_component(&tag) {
        // built-ins are simply fallthroughs / have special handling during ssr
        // so we don't need to import their runtime equivalents
        if !ssr {
            ctx.bindings_helper.helper(built_in.into());
        }
        return VNodeCallTag::Builtin(built_in);
    }

    // 3. User component (from setup bindings)
    // Note: `resolve_component_setup_reference` already handles `.` inside component name
    if let Some(resolved_from_setup) = resolve_component_setup_reference(ctx, &tag) {
        return VNodeCallTag::Expr(resolved_from_setup);
    }

    // 4 & 5 common
    let tag_ident = FervidAtom::from(to_valid_asset_id(&tag, "component")).into_ident();
    ctx.bindings_helper.helper(VueImports::ResolveComponent);

    // 4. Self referencing component (inferred from filename)
    if let Some(self_name) = &ctx.self_name {
        let mut pascal_name = String::with_capacity(tag.len());
        to_pascal_case(&tag, &mut pascal_name);
        if pascal_name.eq(self_name) {
            ctx.bindings_helper.components.insert(
                tag.into_owned(),
                ComponentBinding::RuntimeResolved(
                    Box::new(tag_ident.to_owned()),
                    /* is_self_reference = */ true,
                ),
            );

            return VNodeCallTag::Expr(Box::new(Expr::Ident(tag_ident)));
        }
    }

    // 5. User component (resolve)
    ctx.bindings_helper.components.insert(
        tag.into_owned(),
        ComponentBinding::RuntimeResolved(Box::new(tag_ident.to_owned()), false),
    );
    VNodeCallTag::Expr(Box::new(Expr::Ident(tag_ident)))
}

// TODO: This duplicates `TemplateVisitor::maybe_resolve_component`, clean up the other implementation
/// This is a custom implementation of `resolveSetupReference`
/// which is tailored for components and caches the result in `ctx.bindings_helper.components`
/// for compatibility with the existing codegen.
/// It also utilizes `transform_expr` instead of a fixed transformation
/// for closer integration with the existing transform logic.
fn resolve_component_setup_reference(
    ctx: &mut TransformSfcContext,
    tag_name: &FervidAtom,
) -> Option<Box<Expr>> {
    // Check the existing resolutions.
    // Do nothing if found, regardless if it was previously resolved or not,
    // because codegen will handle the runtime resolution.
    match ctx.bindings_helper.components.get(tag_name) {
        Some(ComponentBinding::Resolved(resolved)) => return Some(resolved.to_owned()),
        Some(ComponentBinding::Unresolved) => return None,
        Some(_) => unreachable!("Only ComponentBinding::Resolved and Unresolved are expected"),
        None => {}
    }

    // If the tag name contains a dot, it won't be found in the bindings - look directly for a namespaced component
    // Example: `<Foo.Bar>`
    let namespace_dot_idx = tag_name.find('.');
    let found = match namespace_dot_idx {
        Some(dot_idx) => find_binding(&mut ctx.bindings_helper, &tag_name[..dot_idx]),
        None => find_binding(&mut ctx.bindings_helper, tag_name),
    };

    if let Some(found) = found {
        let mut resolved_to = Expr::Ident(found.sym.to_owned().into_ident());

        // For `Component` binding types, do not transform.
        // TODO I am not sure about `Imported` though,
        // the official compiler sees them as if `SetupMaybeRef` and transforms.
        if !matches!(found.binding_type, BindingTypes::Component) {
            ctx.bindings_helper
                .transform_expr(&mut resolved_to, ctx.current_template_scope);
        }

        // For namespaced components, add the second part (`Bar` in `<Foo.Bar>`)
        if let Some(dot_idx) = namespace_dot_idx {
            resolved_to = Expr::Member(MemberExpr {
                span: DUMMY_SP,
                obj: Box::new(resolved_to),
                prop: MemberProp::Ident(IdentName {
                    span: DUMMY_SP,
                    sym: FervidAtom::from(&tag_name[(dot_idx + 1)..]),
                }),
            })
        }

        // Was resolved
        ctx.bindings_helper.components.insert(
            tag_name.to_owned(),
            ComponentBinding::Resolved(Box::new(resolved_to.to_owned())),
        );

        Some(Box::new(resolved_to))
    } else {
        // Was not resolved
        ctx.bindings_helper
            .components
            .insert(tag_name.to_owned(), ComponentBinding::Unresolved);

        None
    }
}

fn find_binding<'a>(
    bindings_helper: &'a mut BindingsHelper,
    tag_name: &str,
) -> Option<&'a SetupBinding> {
    // `component-name`s like that should be transformed to `ComponentName`s
    let mut searched_pascal = String::with_capacity(tag_name.len());
    to_pascal_case(tag_name, &mut searched_pascal);

    // and to `componentName`
    let mut searched_camel = String::with_capacity(tag_name.len());
    to_camel_case(tag_name, &mut searched_camel);

    bindings_helper
        .setup_bindings
        .iter()
        .find(|binding| binding.sym == searched_pascal || binding.sym == searched_camel)

    // TODO Auto-importing the components can happen here
}

pub fn build_props(
    node: &ElementNode,
    ctx: &mut TransformSfcContext,
    scope_to_use: u32,
    props: Option<Props>,
    is_component: bool,
    is_dynamic_component: bool,
    ssr: bool,
) -> BuildPropsResult {
    let mut properties: Vec<Property> = vec![];
    let mut merge_args: Vec<PropsExpression> = vec![];
    let mut runtime_directives: Vec<RuntimeDirective> = vec![];
    let element_span = node.span;

    // Patch hints
    // https://github.com/vuejs/core/blob/75220c7995a13a483ae9599a739075be1c8e17f8/packages/compiler-core/src/transforms/transformElement.ts#L395
    let has_children = !node.children.is_empty();
    let mut patch_markers = PatchMarkers::default();

    macro_rules! push_merge_arg {
        ($arg: expr) => {
            push_merge_arg!();
            merge_args.push($arg);
        };
        () => {
            if !properties.is_empty() {
                merge_args.push(PropsExpression::ObjectExpression(Box::new(
                    create_object_expression(dedupe_properties(properties.take()), element_span),
                )));
                // Note: .take() clears properties already
            }
        };
    }

    macro_rules! push_ref_v_for_marker {
        () => {
            if ctx.directive_scopes.v_for > 0 {
                properties.push(create_object_property(
                    create_simple_expression_propname(fervid_atom!("ref_for"), true, DUMMY_SP)
                        .into(),
                    create_simple_expression_bool(true).into(),
                ));
            }
        };
    }

    let props = props.unwrap_or_else(|| Props {
        attributes: &node.starting_tag.attributes,
        directives: node.starting_tag.directives.as_deref(),
    });

    // Cloning transforms is fine here due to the structure being optimized for it
    let transforms = ctx.directive_transforms.clone();

    macro_rules! transform_directive {
        ($transform_name: ident, $value: expr) => {
            if let Some(directive_transform_result) = transforms.$transform_name(ctx, $value, node)
            {
                if !ssr {
                    for prop in directive_transform_result.props.iter() {
                        analyze_patch_flag(
                            &prop,
                            &mut patch_markers,
                            is_component,
                            is_dynamic_component,
                        );
                    }
                }

                properties.extend(directive_transform_result.props);
            }
        };
        ($transform_name: ident, $value: expr, $runtime_variant: ident) => {
            if let Some(directive_transform_result) = transforms.$transform_name(ctx, $value, node)
            {
                if !ssr {
                    for prop in directive_transform_result.props.iter() {
                        analyze_patch_flag(
                            &prop,
                            &mut patch_markers,
                            is_component,
                            is_dynamic_component,
                        );
                    }
                }

                properties.extend(directive_transform_result.props);

                if directive_transform_result.need_runtime {
                    runtime_directives.push(RuntimeDirective::$runtime_variant($value.to_owned()));
                }
            }
        };
    }

    // Static attributes, `v-bind` and `v-on`
    for prop in props.attributes {
        match prop {
            AttributeOrBinding::RegularAttribute { name, value, span } => {
                let mut is_static = true;

                if name == "ref" {
                    patch_markers.has_ref = true;
                    push_ref_v_for_marker!();

                    // TODO: Use `binding_metadata` instead
                    // Get the binding type regardless of template generation mode to mark the ref as "used".
                    // This is the importUsageCheck behavior of the official compiler
                    let binding_type = if value.is_empty() {
                        BindingTypes::Unresolved
                    } else {
                        ctx.bindings_helper
                            .get_var_binding_type(scope_to_use, value)
                    };

                    // https://github.com/vuejs/core/blob/ee4cd78a06e6aa92b12564e527d131d1064c2cd0/packages/compiler-core/src/transforms/transformElement.ts#L506
                    // In inline mode there is no setupState object, so we can't use string
                    // keys to set the ref. Instead, we need to transform it to pass the
                    // actual ref.
                    if !value.is_empty()
                        && ctx.bindings_helper.template_generation_mode.is_inline()
                        && matches!(
                            binding_type,
                            BindingTypes::SetupLet
                                | BindingTypes::SetupRef
                                | BindingTypes::SetupMaybeRef
                                | BindingTypes::Imported
                        )
                    {
                        // TODO: Use value span instead of attr span
                        let value_span = span.to_owned();

                        is_static = false;
                        properties.push(create_object_property(
                            create_simple_expression_propname(
                                fervid_atom!("ref_key"),
                                true,
                                DUMMY_SP,
                            )
                            .into(),
                            create_simple_expression_str(value.to_owned(), true, value_span).into(),
                        ));
                    }
                }

                // skip is on <component>, or is="vue:xxx"
                if name == "is"
                    && (is_component_tag(&node.starting_tag) || value.starts_with("vue:"))
                {
                    continue;
                }

                // TODO: Use name and value spans instead of the attribute span
                let name_span = span.to_owned();
                let value_span = span.to_owned();

                properties.push(create_object_property(
                    create_simple_expression_propname(name.to_owned(), true, name_span).into(),
                    create_simple_expression_str(value.to_owned(), is_static, value_span).into(),
                ));
            }

            AttributeOrBinding::VBind(v_bind_directive) => {
                let arg = v_bind_directive.argument.as_ref();

                // Skip `:is` on <component>
                if is_static_arg_of(arg, "is") && is_component_tag(&node.starting_tag) {
                    continue;
                }

                // https://github.com/vuejs/core/issues/938: elements with dynamic keys should be forced into blocks
                if is_static_arg_of(arg, "key") {
                    patch_markers.should_use_block = true;
                }

                if is_static_arg_of(arg, "ref") {
                    push_ref_v_for_marker!();
                }

                // Special case for v-bind with no argument
                if arg.is_none() {
                    patch_markers.has_dynamic_keys = true;

                    // Expression is guaranteed to be present by the parser

                    // https://github.com/vuejs/core/issues/10696 in case a v-bind object contains ref
                    push_ref_v_for_marker!();
                    push_merge_arg!(v_bind_directive.value.as_ref().into());
                }

                // Force hydration for v-bind with .prop modifier
                if v_bind_directive.is_prop {
                    patch_markers.flags |= PatchFlags::NeedHydration;
                }

                // TODO call the v-bind dispatcher for the directive transform
                // It should decide which function to call for the given context (csr/ssr)
                // For now only add csr and use todo! for ssr
                // Directory structure:
                // - core
                //   - v_bind.rs (needed?)
                // - dom
                //   - v_bind.rs
                // - ssr
                // dynamic_ctx.rs
                //   - fn v_bind_transform(prop, node, context)

                // Optimization: v-bind does not need runtime
                transform_directive!(transform_v_bind, v_bind_directive);
            }

            AttributeOrBinding::VOn(v_on_directive) => {
                // Skip v-on in SSR compilation
                if ssr {
                    continue;
                }

                // Inline before-update hooks need to force block so that it is invoked
                // before children
                if has_children
                    && is_static_arg_of(v_on_directive.event.as_ref(), "vue:before-update")
                {
                    patch_markers.should_use_block = true;
                }

                // Special case for v-on with no argument
                if v_on_directive.event.is_none() {
                    patch_markers.has_dynamic_keys = true;

                    let Some(ref exp) = v_on_directive.handler else {
                        ctx.errors
                            .push(TransformError::TemplateError(TemplateError {
                                kind: TemplateErrorKind::VModelOnProps,
                                span: v_on_directive.span,
                            }));
                        continue;
                    };

                    // Args
                    let mut to_handlers_args = Vec::with_capacity(2);
                    to_handlers_args.push(JsChildNode::ExpressionNode(Box::new(
                        (**exp).to_owned().into(),
                    )));

                    if is_component {
                        to_handlers_args.push(JsChildNode::ExpressionNode(Box::new(
                            ExpressionNode::SimpleExpression(create_simple_expression_bool(true)),
                        )));
                    }

                    // v-on="obj" -> toHandlers(obj)
                    let to_handlers_expr =
                        PropsExpression::CallExpression(Box::new(CallExpression {
                            span: v_on_directive.span,
                            callee: ctx.bindings_helper.helper(VueImports::ToHandlers),
                            arguments: to_handlers_args,
                        }));

                    push_merge_arg!(to_handlers_expr);
                }

                let Some(directive_transform_result) =
                    transforms.transform_v_on(ctx, v_on_directive, node)
                else {
                    continue;
                };

                if !ssr {
                    for prop in directive_transform_result.props.iter() {
                        analyze_patch_flag(
                            prop,
                            &mut patch_markers,
                            is_component,
                            is_dynamic_component,
                        );
                    }
                }

                if let Some(true) = v_on_directive.event.as_ref().map(is_static_exp_str_or_expr) {
                    // Is static
                    properties.extend(directive_transform_result.props);
                } else {
                    push_merge_arg!(PropsExpression::ObjectExpression(Box::new(
                        create_object_expression(directive_transform_result.props, node.span)
                    )));
                };

                // Optimization: v-on does not need runtime
            }
        }
    }

    // Directives
    if let Some(directives) = props.directives {
        // Skip v-once/v-memo - they are handled by dedicated transforms.
        // Skip v-is

        if let Some(ref v_html) = directives.v_html {
            transform_directive!(transform_v_html, v_html);
        }

        if let Some(ref v_text) = directives.v_text {
            transform_directive!(transform_v_text, v_text);
        }

        if let Some(ref v_show) = directives.v_show {
            transform_directive!(transform_v_show, v_show, VShow);
        }

        for v_model in directives.v_model.iter() {
            transform_directive!(transform_v_model, v_model, VModel);
        }

        // Skip v-slot - it is handled by its dedicated transform.
        if let Some(ref _v_slot) = directives.v_slot {
            if !is_component {
                // TODO Add span to v-slot directive
                let span = DUMMY_SP;

                ctx.errors
                    .push(TransformError::TemplateError(TemplateError {
                        span,
                        kind: TemplateErrorKind::VSlotMisplaced,
                    }));
            }
        }

        // User directives
        runtime_directives.reserve(directives.custom.len());
        for user_directive in directives.custom.iter() {
            runtime_directives.push(RuntimeDirective::Custom(user_directive.to_owned()));
            // custom dirs may use beforeUpdate so they need to force blocks
            // to ensure before-update gets called before children update
            if has_children {
                patch_markers.should_use_block = true;
            }
        }
    }

    let mut props_expression: Option<PropsExpression> = None;

    // Has v-bind="object" or v-on="object", wrap with mergeProps
    if !merge_args.is_empty() {
        // Close up any not-yet-merged props
        push_merge_arg!();

        if merge_args.len() > 1 {
            let merge_args = merge_args.into_iter().map(Into::into).collect();

            props_expression = Some(PropsExpression::CallExpression(Box::new(
                create_call_expression(
                    ctx.bindings_helper.helper(VueImports::MergeProps),
                    merge_args,
                    node.span,
                ),
            )));
        } else {
            // Single v-bind with nothing else - no need for a mergeProps call
            props_expression = merge_args.pop();
        }
    } else if !properties.is_empty() {
        props_expression = Some(PropsExpression::ObjectExpression(Box::new(
            create_object_expression(dedupe_properties(properties), node.span),
        )));
    }

    // PatchFlags analysis
    if patch_markers.has_dynamic_keys {
        patch_markers.flags |= PatchFlags::FullProps;
    } else {
        if patch_markers.has_class_binding && !is_component {
            patch_markers.flags |= PatchFlags::Class;
        }
        if patch_markers.has_style_binding && !is_component {
            patch_markers.flags |= PatchFlags::Style;
        }
        if !patch_markers.dynamic_prop_names.is_empty() {
            patch_markers.flags |= PatchFlags::Props;
        }
        if patch_markers.has_hydration_event_binding {
            patch_markers.flags |= PatchFlags::NeedHydration;
        }
    }
    if !patch_markers.should_use_block
        && (patch_markers.flags.is_empty() || patch_markers.flags == PatchFlags::NeedHydration)
        && (patch_markers.has_ref || patch_markers.has_vnode_hook || !runtime_directives.is_empty())
    {
        patch_markers.flags |= PatchFlags::NeedPatch;
    }

    // TODO Use `context.inSSR` instead of `ssr`
    if let Some(props_expression_inner) = props_expression.take_if(|_| !ssr) {
        match props_expression_inner {
            // mergeProps call, do nothing
            PropsExpression::CallExpression(_) => {
                // Because we took the value, put it back
                props_expression = Some(props_expression_inner);
            }

            PropsExpression::ObjectExpression(mut object_lit) => {
                let mut class_prop = None;
                let mut style_prop = None;
                let mut has_dynamic_key = false;

                for prop in object_lit.properties.iter_mut() {
                    let key = &prop.key;

                    if let Some(static_exp) = get_static_exp(key) {
                        if static_exp.ast.sym == "class" {
                            class_prop = Some(prop);
                        } else if static_exp.ast.sym == "style" {
                            style_prop = Some(prop);
                        }
                    } else if !key.is_handler_key() {
                        has_dynamic_key = true;
                    }
                }

                if has_dynamic_key {
                    // Dynamic key binding, wrap with `normalizeProps`
                    let args = vec![JsChildNode::ObjectExpression(object_lit)];

                    props_expression = Some(PropsExpression::CallExpression(Box::new(
                        create_call_expression(
                            ctx.bindings_helper.helper(VueImports::NormalizeProps),
                            args,
                            DUMMY_SP,
                        ),
                    )));
                } else {
                    // No dynamic key
                    if let Some(class_prop) = class_prop {
                        if !is_static_exp(&class_prop.value) {
                            let class_prop_value = std::mem::replace(
                                &mut class_prop.value,
                                create_call_expression(
                                    ctx.bindings_helper.helper(VueImports::NormalizeClass),
                                    Vec::with_capacity(1),
                                    DUMMY_SP,
                                )
                                .into(),
                            );

                            if let JsChildNode::CallExpression(ref mut call_expr) = class_prop.value
                            {
                                call_expr.arguments.push(class_prop_value);
                            }
                        }
                    }

                    if let Some(style_prop) = style_prop {
                        // TODO This does an additional "is array" check, but `has_style_binding` already means `:style` was present.
                        // Here's what `compiler-core` says:
                        // v-bind:style and style both exist,
                        // v-bind:style with static literal object
                        if patch_markers.has_style_binding {
                            let style_prop_value = std::mem::replace(
                                &mut style_prop.value,
                                create_call_expression(
                                    ctx.bindings_helper.helper(VueImports::NormalizeStyle),
                                    Vec::with_capacity(1),
                                    DUMMY_SP,
                                )
                                .into(),
                            );

                            if let JsChildNode::CallExpression(ref mut call_expr) = style_prop.value
                            {
                                call_expr.arguments.push(style_prop_value);
                            }
                        }
                    }

                    // Because we took the value, put it back
                    props_expression = Some(PropsExpression::ObjectExpression(object_lit));
                }
            }

            // Single v-bind
            PropsExpression::ExpressionNode(expr) => {
                let args = vec![JsChildNode::CallExpression(Box::new(
                    create_call_expression(
                        ctx.bindings_helper.helper(VueImports::GuardReactiveProps),
                        vec![JsChildNode::ExpressionNode(expr)],
                        DUMMY_SP,
                    ),
                ))];

                props_expression = Some(PropsExpression::CallExpression(Box::new(
                    create_call_expression(
                        ctx.bindings_helper.helper(VueImports::NormalizeProps),
                        args,
                        DUMMY_SP,
                    ),
                )));
            }
        }
    }

    // TODO Finish the function but keep in mind that `buildProps` is called on node exit,
    // i.e. after all the children have been transformed.
    // I likely have to re-think the scoping mechanism and identifiers injection to accommodate for the change

    // The current design of `v-for` is quite convoluted, but the actual function which is executed is `processFor`
    // which manages the scope and identifiers and processes the code generation on exit,
    // the code generation itself is passed to `processFor` (TODO find a good compromise for supporting CSR + SSR),
    // and the implementation is doing the actual transform on children.

    // For some reason the implementation is surrounded in `createStructuralDirectiveTransform`
    // which basically filters the directive execution to elements only (probably does not apply to Fervid which has visit_element_node)
    // and ignores `vSlot` (why??) plus for some reason the transform is recursive?.. (probably not applying to Fervid which does not allow mutating the tree at random)

    BuildPropsResult {
        patch_hints: PatchHints {
            flags: patch_markers.flags,
            should_use_block: patch_markers.should_use_block,
            props: patch_markers.dynamic_prop_names,
        },
        props: props_expression,
        directives: runtime_directives,
    }
}

fn analyze_patch_flag(
    prop: &Property,
    patch_markers: &mut PatchMarkers,
    is_component: bool,
    is_dynamic_component: bool,
) {
    let Some(key) = get_static_exp(&prop.key) else {
        patch_markers.has_dynamic_keys = true;
        return;
    };

    let name = &key.ast.sym;

    let is_event_handler = is_on(name);
    let is_reserved = is_reserved_prop(name);

    if is_event_handler
        && (!is_component || is_dynamic_component)
        // omit the flag for click handlers because hydration gives click
        // dedicated fast path.
        && !name.eq_ignore_ascii_case("onclick")
        // omit v-model handlers
        && name != "onUpdate:modelValue"
        // omit onVnodeXXX hooks
        && !is_reserved
    {
        patch_markers.has_hydration_event_binding = true;
    }

    if is_event_handler && is_reserved {
        patch_markers.has_vnode_hook = true;
    }

    // TODO
    // if is_event_handler {
    //     if let Some(call_expr) = value.and_then(|v| v.as_call()) {
    //         value = call_expr.args.first().map(|v| &v.expr);
    //     }
    // }

    // if (
    //     value.type === NodeTypes.JS_CACHE_EXPRESSION ||
    //     ((value.type === NodeTypes.SIMPLE_EXPRESSION ||
    //       value.type === NodeTypes.COMPOUND_EXPRESSION) &&
    //       getConstantType(value, context) > 0)
    // ) {
    //     // skip if the prop is a cached handler or has constant value
    //     return
    // }

    match name.as_str() {
        "ref" => patch_markers.has_ref = true,
        "class" => patch_markers.has_class_binding = true,
        "style" => patch_markers.has_style_binding = true,
        x if x != "key" && !patch_markers.dynamic_prop_names.contains(name) => {
            patch_markers.dynamic_prop_names.push(name.to_owned());
        }
        _ => {}
    }

    if is_component
        && (name == "class" || name == "style")
        && !patch_markers.dynamic_prop_names.contains(name)
    {
        patch_markers.dynamic_prop_names.push(name.to_owned());
    }
}

fn dedupe_properties(_properties: Vec<Property>) -> Vec<Property> {
    todo!()
}

fn build_directive_args(_dir: RuntimeDirective, _ctx: &mut TransformSfcContext) -> ArrayLit {
    todo!()
}

fn is_component_tag(tag: &StartingTag) -> bool {
    matches!(tag.tag_name.as_str(), "component" | "Component")
}

// TODO: Move to transform core/utils.rs
fn is_static_exp(arg: &JsChildNode) -> bool {
    matches!(arg, JsChildNode::ExpressionNode(expression_node) if matches!(expression_node.as_ref(), ExpressionNode::SimpleExpression(simple_expression_node) if simple_expression_node.is_static))
}
fn is_static_exp_str_or_expr(arg: &StrOrExpr) -> bool {
    match arg {
        StrOrExpr::Str(_) => true,
        StrOrExpr::Expr(expr) => expr.is_lit(),
    }
}
fn get_static_exp(arg: &ExpressionPropNameNode) -> Option<&SimpleExpressionPropNameNode> {
    match arg {
        ExpressionPropNameNode::SimpleExpression(s) if s.is_static => Some(s),
        _ => None,
    }
}

// TODO Move to ??/general.rs
fn is_on(name: &str) -> bool {
    let mut chars = name.chars();
    matches!((chars.next(), chars.next(), chars.next()), (Some('o'), Some('n'), Some(x)) if x.is_ascii_uppercase())
}

static RESERVED_PROPS: phf::Set<&'static str> = phf_set! {
  "key",
  "ref",
  "ref_for",
  "ref_key",
  "onVnodeBeforeMount",
  "onVnodeMounted",
  "onVnodeBeforeUpdate",
  "onVnodeUpdated",
  "onVnodeBeforeUnmount",
  "onVnodeUnmounted"
};
fn is_reserved_prop(key: &str) -> bool {
    RESERVED_PROPS.contains(key)
}
