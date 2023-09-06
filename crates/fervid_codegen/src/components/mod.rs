use fervid_core::{ElementNode, Node, StartingTag, VSlotDirective, VueDirectives};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::{
        ast::{
            ArrayLit, ArrowExpr, BindingIdent, BlockStmtOrExpr, CallExpr, Callee, Expr,
            ExprOrSpread, Ident, KeyValueProp, Lit, Null, Number, ObjectLit, Pat, Prop,
            PropOrSpread, Str, VarDeclarator,
        },
        atoms::JsWord,
    },
};

use crate::{
    context::CodegenContext, control_flow::SlottedIterator, imports::VueImports,
    utils::str_to_propname,
};

impl CodegenContext {
    pub fn generate_component_vnode(
        &mut self,
        component_node: &ElementNode,
        wrap_in_block: bool,
    ) -> Expr {
        // todo should it be span of the whole component or only of its starting tag?
        let span = DUMMY_SP;

        let component_identifier = Expr::Ident(Ident {
            span,
            sym: self.get_component_identifier(component_node.starting_tag.tag_name),
            optional: false,
        });

        let attributes_obj = self.generate_component_attributes(component_node);
        // TODO Apply all the directives and modifications
        let attributes_expr = if !attributes_obj.props.is_empty() {
            Some(Expr::Object(attributes_obj))
        } else {
            None
        };

        let children_slots = self.generate_component_children(component_node);

        // TODO Use the correct patch flag
        let patch_flags = 0;

        // Call the general constructor
        let mut create_component_expr = self.generate_componentlike(
            component_identifier,
            attributes_expr,
            children_slots,
            patch_flags,
            wrap_in_block,
            span,
        );

        // Process directives
        create_component_expr =
            self.generate_component_directives(create_component_expr, component_node);

        create_component_expr
    }

    // Generates a `createVNode`/`createBlock` structure
    pub fn generate_componentlike(
        &mut self,
        identifier: Expr,
        attributes: Option<Expr>,
        children_or_slots: Option<Expr>,
        patch_flag: i32,
        wrap_in_block: bool,
        span: Span,
    ) -> Expr {
        // Wire the things together. `createVNode` args:
        // 1st - component identifier;
        // 2nd (optional) - component attributes & directives object;
        // 3rd (optional) - component slots;
        // 4th (optional) - component patch flag.
        let expected_component_args_count = if patch_flag != 0 {
            4
        } else if children_or_slots.is_some() {
            3
        } else if attributes.is_some() {
            2
        } else {
            1
        };

        // Arguments for function call
        let mut create_component_args: Vec<ExprOrSpread> =
            Vec::with_capacity(expected_component_args_count);

        // Arg 1: component identifier
        create_component_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(identifier),
        });

        // Arg 2 (optional): component attributes expression (default to null)
        if expected_component_args_count >= 2 {
            let expr_to_push = if let Some(attributes_expr) = attributes {
                Box::new(attributes_expr)
            } else {
                null(span)
            };
            create_component_args.push(ExprOrSpread {
                spread: None,
                expr: expr_to_push,
            });
        }

        // Arg 3 (optional): component children expression (default to null)
        if expected_component_args_count >= 3 {
            let expr_to_push = children_or_slots.map_or_else(|| null(span), Box::new);
            create_component_args.push(ExprOrSpread {
                spread: None,
                expr: expr_to_push,
            })
        }

        // Arg 4 (optional): patch flags (default to nothing)
        if expected_component_args_count >= 4 {
            create_component_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: patch_flag as f64,
                    raw: None,
                }))),
            })
        }

        // When wrapping in block, `createBlock` is used, otherwise `createVNode`
        let create_component_fn_ident = self.get_and_add_import_ident(if wrap_in_block {
            VueImports::CreateBlock
        } else {
            VueImports::CreateVNode
        });

        // `createVNode(_component_name, {component:attrs}, {component:slots}, PATCH_FLAGS)`
        let create_component_fn_call = Expr::Call(CallExpr {
            span,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                span,
                sym: create_component_fn_ident,
                optional: false,
            }))),
            args: create_component_args,
            type_args: None,
        });

        // When wrapping in block, we also need `openBlock()`
        

        if wrap_in_block {
            // (openBlock(), createBlock(_component_name, {component:attrs}, {component:slots}, PATCH_FLAGS))
            self.wrap_in_open_block(create_component_fn_call, span)
        } else {
            // Just `createVNode` call
            create_component_fn_call
        }
    }

    pub fn generate_component_resolves(&mut self) -> Vec<VarDeclarator> {
        let mut result = Vec::new();

        if self.components.is_empty() {
            return result;
        }

        let resolve_component_ident = self.get_and_add_import_ident(VueImports::ResolveComponent);

        // We need sorted entries for stable output.
        // Entries are sorted by Js identifier (second element of tuple in hashmap entry)
        let mut sorted_components: Vec<(&str, &JsWord)> = self
            .components
            .iter()
            .map(|(component_name, component_ident)| (component_name.as_str(), component_ident))
            .collect();

        sorted_components.sort_by(|a, b| a.1.cmp(b.1));

        // Key is a component as used in template, value is the assigned Js identifier
        for (component_name, identifier) in sorted_components.iter() {
            // _component_ident_name = resolveComponent("component-name")
            result.push(VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident {
                        span: DUMMY_SP,
                        sym: (*identifier).to_owned(),
                        optional: false,
                    },
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Call(CallExpr {
                    span: DUMMY_SP,
                    callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                        span: DUMMY_SP,
                        sym: resolve_component_ident.to_owned(),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        expr: Box::new(Expr::Lit(Lit::Str(Str {
                            span: DUMMY_SP,
                            value: JsWord::from(*component_name),
                            raw: None,
                        }))),
                    }],
                    type_args: None,
                }))),
                definite: false,
            });
        }

        result
    }

    fn generate_component_attributes(&mut self, component_node: &ElementNode) -> ObjectLit {
        let mut result_props = Vec::new();

        self.generate_attributes(&component_node.starting_tag.attributes, &mut result_props);

        // Process directives
        if let Some(ref directives) = component_node.starting_tag.directives {
            // `v-model`s
            for v_model in directives.v_model.iter() {
                self.generate_v_model_for_component(
                    v_model,
                    &mut result_props,
                    component_node.template_scope,
                );
            }

            // Process `v-text`
            if let Some(ref v_text) = directives.v_text {
                result_props.push(self.generate_v_text(v_text));
            }

            // Process `v-html`
            if let Some(ref v_html) = directives.v_html {
                result_props.push(self.generate_v_html(v_html));
            }
        }

        // TODO Take the remaining_directives and call a forwarding function
        // Process directives and hints wrt the createVNode

        

        ObjectLit {
            span: DUMMY_SP, // todo from the component_node
            props: result_props,
        }
    }

    pub(crate) fn generate_component_children(&mut self, component_node: &ElementNode) -> Option<Expr> {
        let mut result_static_slots = Vec::new();
        let total_children = component_node.children.len();

        // No children work, return immediately
        if total_children == 0 {
            return None;
        }

        // Prepare the necessities.
        let component_span = DUMMY_SP; // todo
        let mut default_slot_children: Vec<Expr> = Vec::new();

        // `SlottedIterator` will iterate over sequences of default or named slots,
        // and it will stop yielding elements unless [`SlottedIterator::toggle_mode`] is called.
        let mut slotted_iterator = SlottedIterator::new(&component_node.children);

        // Whether the default slot element was encountered
        // This is needed to avoid situation like that:
        // <some-component>
        //   <template v-slot:default="{ value }">{{ value }}</template>
        //   not hi
        // </some-component>
        //
        // We cannot really compile such templates,
        // because `value` is only available to the first element.
        //
        // TODO But we can optimize it and put all the children inside the first <template>:
        // <some-component>
        //   <template v-slot:default="{ value }">
        //     {{ value }}
        //     not hi
        //   </template>
        // </some-component>
        // This needs an RFC
        let mut has_encountered_default_slot = false;
        // let mut default_slot_is_not_template = false;

        // Generate the default slot items into the `default_slot_children` vec,
        // and named slots into the `result` vec.
        while slotted_iterator.has_more() {
            // Generate either the default slot child, or `<template v-slot:default>`
            if slotted_iterator.is_default_slot_mode() {
                let Some(node) = slotted_iterator.peek() else {
                    slotted_iterator.toggle_mode();
                    continue;
                };

                // Default slot children.
                // We generate a sequence only if we know that
                // the component has multiple children not belonging to a `<template>`,
                // e.g. `<some-component><span>Hi</span>there</some-component>`.
                if has_encountered_default_slot {
                    self.generate_node_sequence(
                        &mut slotted_iterator,
                        &mut default_slot_children,
                        total_children,
                        false,
                    );
                    slotted_iterator.toggle_mode_if_peek_is_none();
                    continue;
                }

                has_encountered_default_slot = true;

                // Check if we found a `<template v-slot:default>` or `<template v-slot>`
                // If we found it, we are in a similar case as if it was a named template.

                macro_rules! not_in_a_template_v_slot {
                    () => {
                        continue;
                    };
                }

                // Check if this is a `<template>` or not
                let Node::Element(
                    ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            directives: Some(directives),
                            ..
                        },
                        children,
                        ..
                    }) = node else {
                    not_in_a_template_v_slot!();
                };

                // Check `v-slot` existence
                let Some(ref v_slot_directive) = directives.v_slot else {
                    not_in_a_template_v_slot!();
                };

                // At this point, we have `<template v-slot="maybeSomeBinding">`
                // We need to generate it as if it was a named slot
                self.generate_named_slot(
                    v_slot_directive,
                    children,
                    directives,
                    &mut result_static_slots,
                );

                // Advance the iterator forward
                slotted_iterator.next();
            } else {
                // Generate the slotted child
                let Some(slotted_node) = slotted_iterator.next() else {
                    slotted_iterator.toggle_mode();
                    continue;
                };

                let Node::Element(slotted_node) = slotted_node else {
                    unreachable!("Only element node can be slotted")
                };

                // Get `v-slot`
                let Some(ref directives) = slotted_node.starting_tag.directives else {
                    unreachable!("Slotted node should have a v-slot directive");
                };
                let Some(ref v_slot_directive) = directives.v_slot else {
                    unreachable!("Slotted node should have a v-slot directive");
                };

                // TODO Components with v-slot are not supported yet?..

                self.generate_named_slot(
                    v_slot_directive,
                    &slotted_node.children,
                    directives,
                    &mut result_static_slots,
                );
            }

            slotted_iterator.toggle_mode();
        }

        // Add default slot children when needed
        // TODO Error on cases when both `<template v-slot:default>`
        // and non-slotted children are present (analyzer), e.g.:
        // <some-component>
        //   <template v-slot:default>hi</template>
        //   not hi
        // </some-component>
        if !default_slot_children.is_empty() {
            // withCtx(() => [child1, child2, child3])
            result_static_slots.push(self.generate_slot_shell(
                "default",
                default_slot_children,
                None, // todo get the binding for `<template v-slot="binding"`
                component_span,
            ));
        }

        // TODO Add `createSlots` if needed
        Some(Expr::Object(ObjectLit {
            span: component_span,
            props: result_static_slots,
        }))
    }

    /// Generates a named slot using a vector of slot children.
    /// Primarily for `<template v-slot:named>` or `<template v-slot:default>`
    fn generate_named_slot(
        &mut self,
        v_slot: &VSlotDirective,
        slot_children: &[Node],
        directives: &VueDirectives,
        out_static_slots: &mut Vec<PropOrSpread>,
    ) {
        // Extra logic is needed if this is more than just `<template v-slot>`
        let is_complex = directives.v_if.is_some()
            || directives.v_else_if.is_some()
            || directives.v_else.is_some()
            || directives.v_for.is_some();

        if is_complex {
            todo!("createSlots is not supported yet");
            // https://play.vuejs.org/#eNqVVNtuozAQ/ZUpWolGKo2y+xaRqFVfdr9gH0qlOngIVo2NbEMbRfz7js0l0G33okTgmfGcMzfmHN3X9W3bYLSNUpsbUTuw6JoaJFPHXRY5m0X7TImq1sbBGQwW0EFhdAUxucWZylSulXVQ2SPsvP06/o5SavipjeRX8SpT6bqHJiASBpolQY/xWjKHLRoCiktB4PgWeDkWrJHEnykAzhy7XvVnIELXGDVK4MOwW+hDIACv6vyLHvRfRuKwqiUxkgSQctFCm1RYoQ0KyFxabvbnc8CErgM6ThF2Xboma8+QClU3zntrjpKyIo+QVjDypqpOSU5mrchGmWYRCQehOIkH3SjuNXe5FPkLab4QgXKw20PJFJf44PXXvXYVYDPnf+lVksC9lOBKBE0PA1wYzJ1o0QIzCOKotEEOooDnNhHFMwgLhSe8hR/8xeuFiy3k2ng/SJIhZop6LA5FaqV226EH0wWAUkyX14tSfuj/yE+KVSJ/miH4aoLSr3/A+RSPGmXpLAo/RKZBKsy/hBH6SwJKi72zAKFgM7XL5/UbULoemjj1NAzLjLwfHU3y49MoeVKt8v7lp8JzB6PDN0cKqmAE6zCMAbXcUHc8xoEZslxqNt69m2vm8tdBMTCNY7qEr2HLhWUHiX7wiGmWdGprpvalODH6RvzxUg0/XwZHFOhHpiLFskM+5qFY9eA98bZJoc1Y628hznnjLlxLBCq7aOk060d0Ez3o6v3OWi4TCnHcVzS1QqH30Mp/VO9310cLhgqKtELKsEJu+rz/unTI4f/WzbRa5qvk00Uyr0D3Cy9D1W8=
            // idk if that should be wrapped in block or not
            // _createSlots({
            //     default: _withCtx(() => [
            //       _createTextVNode(" hi ")
            //     ]),
            //     _: 2 /* DYNAMIC */
            //   }, [
            //     _renderList(1, (i) => {
            //       return {
            //         name: "memes",
            //         fn: _withCtx(() => [
            //           _createTextVNode(" hi")
            //         ])
            //       }
            //     })
            //   ]), 1040 /* FULL_PROPS, DYNAMIC_SLOTS */)
            // let generated = self.generate_node(slotted_node, false);
        } else {
            // Generate the children of the `<template v-slot>`
            let total_children = slot_children.len();
            let mut slotted_children_results = Vec::with_capacity(total_children);
            let mut slotted_children_iter = slot_children.iter();

            self.generate_node_sequence(
                &mut slotted_children_iter,
                &mut slotted_children_results,
                total_children,
                false,
            );

            let slot_name = v_slot.slot_name.unwrap_or("default");
            let span = DUMMY_SP; // todo?

            out_static_slots.push(self.generate_slot_shell(
                slot_name,
                slotted_children_results,
                v_slot.value.as_deref(),
                span,
            ));
        }
    }

    /// Creates the SWC identifier from a tag name. Will fetch from cache if present
    fn get_component_identifier(&mut self, tag_name: &str) -> JsWord {
        // Cached
        let existing_component_name = self.components.get(tag_name);
        if let Some(component_name) = existing_component_name {
            return component_name.to_owned();
        }

        // _component_ prefix plus tag name
        let mut component_name = tag_name.replace('-', "_");
        component_name.insert_str(0, "_component_");

        // To create an identifier, we need to convert it to an SWC JsWord
        let component_name = JsWord::from(component_name);

        self.components
            .insert(tag_name.to_owned(), component_name.to_owned());

        component_name
    }

    // Generates `withDirectives(expr, [directives])`
    fn generate_component_directives(
        &mut self,
        create_component_expr: Expr,
        component_node: &ElementNode,
    ) -> Expr {
        // Guard because we need the whole `ElementNode`, not just `VueDirectives`
        let Some(ref directives) = component_node.starting_tag.directives else {
            return create_component_expr;
        };

        // Output array for `withDirectives` call.
        // If this stays empty at the end, no processing to `create_element_expr` would be done
        let mut out: Vec<Option<ExprOrSpread>> = Vec::new();

        self.generate_directives_to_array(directives, &mut out);
        self.maybe_generate_with_directives(create_component_expr, out)
    }

    /// Generates `_slotName_: withCtx((_maybeCtx_) => [slot, children])`
    fn generate_slot_shell(
        &mut self,
        slot_name: &str,
        slot_children: Vec<Expr>,
        slot_binding: Option<&Pat>,
        span: Span,
    ) -> PropOrSpread {
        // e.g. child1, child2, child3
        let children_elems = slot_children
            .into_iter()
            .map(|child| {
                Some(ExprOrSpread {
                    spread: None,
                    expr: Box::new(child),
                })
            })
            .collect();

        // [child1, child2, child3]
        let children_arr = ArrayLit {
            span,
            elems: children_elems,
        };

        // Params to arrow function.
        // `withCtx(() => /*...*/)` or `withCtx(({ maybe: destructure }) => /*...*/)`
        let params = if let Some(slot_binding) = slot_binding {
            vec![slot_binding.to_owned()]
        } else {
            Vec::new()
        };

        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: str_to_propname(slot_name, span),
            value: Box::new(Expr::Call(CallExpr {
                span,
                // withCtx
                callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                    span,
                    sym: self.get_and_add_import_ident(VueImports::WithCtx),
                    optional: false,
                }))),
                args: vec![ExprOrSpread {
                    spread: None,
                    // () => [child1, child2, child3]
                    expr: Box::new(Expr::Arrow(ArrowExpr {
                        span,
                        params,
                        body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Array(children_arr)))),
                        is_async: false,
                        is_generator: false,
                        type_params: None,
                        return_type: None,
                    })),
                }],
                type_args: None,
            })),
        })))
    }
}

#[inline]
fn null(span: Span) -> Box<Expr> {
    Box::new(Expr::Lit(Lit::Null(Null { span })))
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        AttributeOrBinding, ElementKind, Interpolation, Node, StartingTag, VBindDirective,
    };

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_basic_usage() {
        // <test-component></test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r"_createVNode(_component_test_component)",
            false,
        );

        // <test-component />
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r"_createVNode(_component_test_component)",
            false,
        );
    }

    #[test]
    fn it_generates_attributes() {
        // <test-component foo="bar" :baz="qux"></test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![
                        AttributeOrBinding::RegularAttribute {
                            name: "foo",
                            value: "bar",
                        },
                        AttributeOrBinding::VBind(VBindDirective {
                            argument: Some("some-baz".into()),
                            value: js("qux"),
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        }),
                    ],
                    directives: None,
                },
                children: vec![],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,{foo:"bar","some-baz":qux})"#,
            false,
        );
    }

    #[test]
    fn it_generates_default_slot() {
        // <test-component>hello from component<div>hello from div</div></test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Text("hello from component"),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "div",
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello from div")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"default":_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")])})"#,
            false,
        );

        // <test-component>
        //   <tempate v-slot:default>hello from component<div>hello from div</div></template>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![Node::Element(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "template",
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("default"),
                                value: None,
                                is_dynamic_slot: false,
                            }),
                            ..Default::default()
                        })),
                    },
                    children: vec![
                        Node::Text("hello from component"),
                        Node::Element(ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "div",
                                attributes: vec![],
                                directives: None,
                            },
                            children: vec![Node::Text("hello from div")],
                            template_scope: 0,
                            kind: ElementKind::Element,
                        }),
                    ],
                    template_scope: 0,
                    kind: ElementKind::Element,
                })],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"default":_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")])})"#,
            false,
        );
    }

    #[test]
    fn it_generates_named_slot() {
        // <test-component>
        //   <tempate v-slot:foo-bar>hello from component<div>hello from div</div></template>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![Node::Element(ElementNode {
                    starting_tag: StartingTag {
                        tag_name: "template",
                        attributes: vec![],
                        directives: Some(Box::new(VueDirectives {
                            v_slot: Some(VSlotDirective {
                                slot_name: Some("foo-bar"),
                                value: None,
                                is_dynamic_slot: false,
                            }),
                            ..Default::default()
                        })),
                    },
                    children: vec![
                        Node::Text("hello from component"),
                        Node::Element(ElementNode {
                            starting_tag: StartingTag {
                                tag_name: "div",
                                attributes: vec![],
                                directives: None,
                            },
                            children: vec![Node::Text("hello from div")],
                            template_scope: 0,
                            kind: ElementKind::Element,
                        }),
                    ],
                    template_scope: 0,
                    kind: ElementKind::Element,
                })],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")])})"#,
            false,
        );
    }

    #[test]
    fn it_generates_multiple_named_slots() {
        // <test-component>
        //   <tempate v-slot:foo-bar>hello from slot {{ one }}</template>
        //   <tempate v-slot:baz>hello from slot <b>two</b></template>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("foo-bar"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![
                            Node::Text("hello from slot "),
                            Node::Interpolation(Interpolation {
                                value: js("one"),
                                template_scope: 0,
                                patch_flag: true,
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("baz"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![
                            Node::Text("hello from slot "),
                            Node::Element(ElementNode {
                                starting_tag: StartingTag {
                                    tag_name: "b",
                                    attributes: vec![],
                                    directives: None,
                                },
                                children: vec![Node::Text("two")],
                                template_scope: 0,
                                kind: ElementKind::Element,
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot "+_toDisplayString(one),1)]),baz:_withCtx(()=>[_createTextVNode("hello from slot "),_createElementVNode("b",null,"two")])})"#,
            false,
        );
    }

    #[test]
    fn it_generates_mixed_slots() {
        // <test-component>
        //   hello from component<div>hello from div</div>
        //   <tempate v-slot:foo-bar>hello from slot</template>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Text("hello from component"),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "div",
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello from div")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("foo-bar"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![Node::Text("hello from slot")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),"default":_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")])})"#,
            false,
        );

        // <test-component>
        //   <template v-slot>hello from default<div>hello from div</div></template>
        //   <tempate v-slot:foo-bar>hello from slot</template>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: None,
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![
                            Node::Text("hello from default"),
                            Node::Element(ElementNode {
                                starting_tag: StartingTag {
                                    tag_name: "div",
                                    attributes: vec![],
                                    directives: None,
                                },
                                children: vec![Node::Text("hello from div")],
                                template_scope: 0,
                                kind: ElementKind::Element,
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("foo-bar"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![Node::Text("hello from slot")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"default":_withCtx(()=>[_createTextVNode("hello from default"),_createElementVNode("div",null,"hello from div")]),"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")])})"#,
            false,
        );

        // <test-component>
        //   <tempate v-slot:foo-bar>hello from slot</template>
        //   hello from component<div>hello from div</div>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("foo-bar"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![Node::Text("hello from slot")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                    Node::Text("hello from component"),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "div",
                            attributes: vec![],
                            directives: None,
                        },
                        children: vec![Node::Text("hello from div")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),"default":_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")])})"#,
            false,
        );
    }

    #[test]
    fn it_generates_mixed_slots_multiple() {
        // <test-component>
        //   <tempate v-slot:foo-bar>hello from slot</template>
        //   <template v-slot>hello from default<div>hello from div</div></template>
        //   <tempate v-slot:baz>hello from baz</template>
        // </test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    directives: None,
                },
                children: vec![
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("foo-bar"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![Node::Text("hello from slot")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: None,
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![
                            Node::Text("hello from default"),
                            Node::Element(ElementNode {
                                starting_tag: StartingTag {
                                    tag_name: "div",
                                    attributes: vec![],
                                    directives: None,
                                },
                                children: vec![Node::Text("hello from div")],
                                template_scope: 0,
                                kind: ElementKind::Element,
                            }),
                        ],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                    Node::Element(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes: vec![],
                            directives: Some(Box::new(VueDirectives {
                                v_slot: Some(VSlotDirective {
                                    slot_name: Some("baz"),
                                    value: None,
                                    is_dynamic_slot: false,
                                }),
                                ..Default::default()
                            })),
                        },
                        children: vec![Node::Text("hello from baz")],
                        template_scope: 0,
                        kind: ElementKind::Element,
                    }),
                ],
                template_scope: 0,
                kind: ElementKind::Component,
            },
            r#"_createVNode(_component_test_component,null,{"foo-bar":_withCtx(()=>[_createTextVNode("hello from slot")]),"default":_withCtx(()=>[_createTextVNode("hello from default"),_createElementVNode("div",null,"hello from div")]),baz:_withCtx(()=>[_createTextVNode("hello from baz")])})"#,
            false,
        );
    }

    fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_component_vnode(&input, wrap_in_block);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
