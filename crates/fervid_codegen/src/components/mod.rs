use fervid_core::{
    ElementNode, HtmlAttribute, Node, StartingTag, VDirective, VForDirective, VSlotDirective,
};
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::{
        ast::{
            ArrayLit, ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread, Ident,
            KeyValueProp, Lit, Null, Number, ObjectLit, Pat, Prop, PropName, PropOrSpread,
        },
        atoms::{js_word, JsWord},
    },
};

use crate::{
    attributes::DirectivesToProcess, context::CodegenContext, control_flow::SlottedIterator,
    imports::VueImports, utils::str_to_propname,
};

/// Describes the `v-slot`, `v-for`, `v-if`,
/// `v-else-if`, `v-else` directives supported by <template>
#[derive(Default)]
struct TemplateDirectives<'d> {
    v_slot: Option<&'d VSlotDirective<'d>>,
    v_for: Option<&'d VForDirective<'d>>,
    v_if: Option<&'d str>,
    v_else_if: Option<&'d str>,
    v_else: Option<()>,
}

impl CodegenContext {
    pub fn generate_component_vnode(
        &mut self,
        component_node: &ElementNode,
        wrap_in_block: bool,
    ) -> Expr {
        // TODO how?..
        let needs_patch_flags = false;
        // todo should it be span of the whole component or only of its starting tag?
        let span = DUMMY_SP;

        let (attributes_obj, remaining_directives) =
            self.generate_component_attributes(component_node);

        // TODO Apply all the directives and modifications
        let attributes_expr = if attributes_obj.props.len() != 0 {
            Some(Expr::Object(attributes_obj))
        } else {
            None
        };

        let children_slots = self.generate_component_children(component_node);

        // Wire the things together. `createVNode` args:
        // 1st - component identifier;
        // 2nd (optional) - component attributes & directives object;
        // 3rd (optional) - component slots;
        // 4th (optional) - component patch flag.
        let expected_component_args_count = if needs_patch_flags {
            4
        } else if children_slots.is_some() {
            3
        } else if let Some(_) = attributes_expr {
            2
        } else {
            1
        };

        // Arguments for function call
        let mut create_component_args = Vec::with_capacity(expected_component_args_count);

        /// Produces a `null` expression
        macro_rules! null {
            () => {
                Box::new(Expr::Lit(Lit::Null(Null { span })))
            };
        }

        // Arg 1: component identifier
        create_component_args.push(ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(Ident {
                span,
                sym: self.get_component_identifier(component_node.starting_tag.tag_name),
                optional: false,
            })),
        });

        // Arg 2 (optional): component attributes expression (default to null)
        if expected_component_args_count >= 2 {
            let expr_to_push = if let Some(attributes_expr) = attributes_expr {
                Box::new(attributes_expr)
            } else {
                null!()
            };
            create_component_args.push(ExprOrSpread {
                spread: None,
                expr: expr_to_push,
            });
        }

        // Arg 3 (optional): component children expression (default to null)
        if expected_component_args_count >= 3 {
            let expr_to_push = children_slots.map_or_else(|| null!(), |expr| Box::new(expr));
            create_component_args.push(ExprOrSpread {
                spread: None,
                expr: expr_to_push,
            })
        }

        // Arg 4 (optional): patch flags (default to nothing)
        if expected_component_args_count >= 4 {
            // TODO Actual patch flag value
            create_component_args.push(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: 512.0, // TODO
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
        let mut create_component_expr = if wrap_in_block {
            // (openBlock(), createBlock(_component_name, {component:attrs}, {component:slots}, PATCH_FLAGS))
            self.wrap_in_open_block(create_component_fn_call, span)
        } else {
            // Just `createVNode` call
            create_component_fn_call
        };

        // Process remaining directives
        if remaining_directives.len() != 0 {
            self.generate_remaining_component_directives(
                &mut create_component_expr,
                &remaining_directives,
            );
        }

        create_component_expr
    }

    fn generate_component_attributes<'e>(
        &mut self,
        component_node: &'e ElementNode,
    ) -> (ObjectLit, DirectivesToProcess<'e>) {
        let mut result_props = Vec::new();
        let mut remaining_directives = DirectivesToProcess::new();

        self.generate_attributes(
            &component_node.starting_tag.attributes,
            &mut result_props,
            &mut remaining_directives,
            component_node.template_scope,
        );

        // Process v-models
        remaining_directives.retain(|directive| match directive {
            VDirective::Model(v_model) => {
                self.generate_v_model_for_component(
                    v_model,
                    &mut result_props,
                    component_node.template_scope,
                );
                false
            }

            _ => true,
        });

        // TODO Take the remaining_directives and call a forwarding function
        // Process directives and hints wrt the createVNode

        let result = ObjectLit {
            span: DUMMY_SP, // todo from the component_node
            props: result_props,
        };

        (result, remaining_directives)
    }

    fn generate_component_children(&mut self, component_node: &ElementNode) -> Option<Expr> {
        let mut result_static_slots = Vec::new();
        let total_children = component_node.children.len();

        // No children work, return immediately
        if total_children == 0 {
            return None;
        }

        // Prepare the necessities.
        let component_span = DUMMY_SP; // todo
        let mut default_slot_children: Vec<Expr> = Vec::new();
        let with_ctx_ident = self.get_and_add_import_ident(VueImports::WithCtx);

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
                    continue;
                }

                has_encountered_default_slot = true;

                // Check if we found a `<template v-slot:default>` or `<template v-slot>`
                // If we found it, we are in a similar case as if it was a named template.

                let Some(node) = slotted_iterator.peek() else {
                    slotted_iterator.toggle_mode();
                    continue;
                };

                macro_rules! not_in_a_template_v_slot {
                    () => {
                        continue;
                    };
                }

                // Check if this is a `<template>` or not
                let Node::ElementNode(
                    ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "template",
                            attributes,
                            ..
                        },
                        children,
                        template_scope
                    }) = node else {
                    not_in_a_template_v_slot!();
                };

                // Find all the supported directives
                let supported_directives = find_template_supported_directives(attributes);

                // Check `v-slot` existence
                let Some(v_slot_directive) = supported_directives.v_slot else {
                    not_in_a_template_v_slot!();
                };

                // At this point, we have `<template v-slot="maybeSomeBinding">`
                // We need to generate it as if it was a named slot
                self.generate_named_slot(
                    v_slot_directive,
                    &children,
                    *template_scope,
                    &supported_directives,
                    &mut result_static_slots,
                );
            } else {
                // Generate the slotted child
                let Some(slotted_node) = slotted_iterator.next() else {
                    slotted_iterator.toggle_mode();
                    continue;
                };

                let Node::ElementNode(slotted_node) = slotted_node else {
                    unreachable!("Only element node can be slotted")
                };

                let supported_directives =
                    find_template_supported_directives(&slotted_node.starting_tag.attributes);

                // Check `v-slot` existence
                let Some(v_slot_directive) = supported_directives.v_slot else {
                    unreachable!("Slotted node should have a v-slot directive");
                };

                // TODO Components with v-slot are not supported yet?..

                self.generate_named_slot(
                    v_slot_directive,
                    &slotted_node.children,
                    slotted_node.template_scope,
                    &supported_directives,
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
        if default_slot_children.len() != 0 {
            // withCtx(() => [child1, child2, child3])
            result_static_slots.push(self.generate_slot_shell(
                js_word!("default"),
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
        slot_scope: u32,
        template_directives: &TemplateDirectives,
        out_static_slots: &mut Vec<PropOrSpread>,
    ) {
        // Extra logic is needed if this is more than just `<template v-slot>`
        let is_complex = template_directives.v_if.is_some()
            || template_directives.v_else_if.is_some()
            || template_directives.v_else.is_some()
            || template_directives.v_for.is_some();

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

            let slot_name = JsWord::from(v_slot.slot_name.unwrap_or("default"));
            let span = DUMMY_SP; // todo?

            out_static_slots.push(self.generate_slot_shell(
                slot_name,
                slotted_children_results,
                None,
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

        return component_name;
    }

    // Generates `withDirectives(expr, [directives])`
    fn generate_remaining_component_directives(
        &mut self,
        create_component_expr: &mut Expr,
        remaining_directives: &DirectivesToProcess,
    ) {
        self.generate_remaining_directives(create_component_expr, remaining_directives)
    }

    /// Generates `_slotName_: withCtx((_maybeCtx_) => [slot, children])`
    fn generate_slot_shell(
        &mut self,
        slot_name: JsWord,
        slot_children: Vec<Expr>,
        slot_binding: Option<Pat>,
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
            vec![slot_binding]
        } else {
            Vec::new()
        };

        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(Ident {
                span,
                sym: slot_name,
                optional: false,
            }),
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

/// Finds the `v-slot`, `v-for`, `v-if`, `v-else-if`, `v-else` directives
fn find_template_supported_directives<'e>(
    attributes: &'e [HtmlAttribute],
) -> TemplateDirectives<'e> {
    let mut result = TemplateDirectives::default();

    for attr in attributes.iter() {
        match attr {
            HtmlAttribute::VDirective(VDirective::If(cond)) => result.v_if = Some(&cond),
            HtmlAttribute::VDirective(VDirective::ElseIf(cond)) => result.v_else_if = Some(&cond),
            HtmlAttribute::VDirective(VDirective::Else) => result.v_else = Some(()),
            HtmlAttribute::VDirective(VDirective::For(v_for)) => result.v_for = Some(v_for),
            HtmlAttribute::VDirective(VDirective::Slot(v_slot)) => result.v_slot = Some(v_slot),
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use fervid_core::{HtmlAttribute, Node, StartingTag};

    use super::*;

    #[test]
    fn it_generates_basic_usage() {
        // <test-component></test-component>
        test_out(
            ElementNode {
                starting_tag: StartingTag {
                    tag_name: "test-component",
                    attributes: vec![],
                    is_self_closing: false,
                    kind: fervid_core::ElementKind::Normal,
                },
                children: vec![],
                template_scope: 0,
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
                    is_self_closing: true,
                    kind: fervid_core::ElementKind::Normal,
                },
                children: vec![],
                template_scope: 0,
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
                        HtmlAttribute::Regular {
                            name: "foo",
                            value: "bar",
                        },
                        HtmlAttribute::VDirective(VDirective::Bind(fervid_core::VBindDirective {
                            argument: Some("some-baz"),
                            value: "qux",
                            is_dynamic_attr: false,
                            is_camel: false,
                            is_prop: false,
                            is_attr: false,
                        })),
                    ],
                    is_self_closing: false,
                    kind: fervid_core::ElementKind::Normal,
                },
                children: vec![],
                template_scope: 0,
            },
            r#"_createVNode(_component_test_component,{foo:"bar","some-baz":_ctx.qux})"#,
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
                    is_self_closing: false,
                    kind: fervid_core::ElementKind::Normal,
                },
                children: vec![
                    Node::TextNode("hello from component"),
                    Node::ElementNode(ElementNode {
                        starting_tag: StartingTag {
                            tag_name: "div",
                            attributes: vec![],
                            is_self_closing: false,
                            kind: fervid_core::ElementKind::Normal,
                        },
                        children: vec![Node::TextNode("hello from div")],
                        template_scope: 0,
                    }),
                ],
                template_scope: 0,
            },
            r#"_createVNode(_component_test_component,null,{default:_withCtx(()=>[_createTextVNode("hello from component"),_createElementVNode("div",null,"hello from div")])})"#,
            false,
        );
    }

    fn test_out(input: ElementNode, expected: &str, wrap_in_block: bool) {
        let mut ctx = CodegenContext::default();
        let out = ctx.generate_component_vnode(&input, wrap_in_block);
        assert_eq!(crate::test_utils::to_str(out), expected)
    }
}
