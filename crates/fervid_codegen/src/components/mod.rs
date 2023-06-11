use fervid_core::{ElementNode, VDirective};
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{
            ArrayLit, ArrowExpr, BlockStmtOrExpr, CallExpr, Callee, Expr, ExprOrSpread, Ident,
            KeyValueProp, Lit, Null, Number, ObjectLit, ParenExpr, Prop, PropOrSpread, SeqExpr,
        },
        atoms::{js_word, JsWord},
    },
};

use crate::{
    attributes::DirectivesToProcess, context::CodegenContext, control_flow::SlottedIterator,
    imports::VueImports,
};

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
        } else if children_slots.len() != 0 {
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
            let expr_to_push = if children_slots.len() != 0 {
                Box::new(Expr::Object(ObjectLit {
                    span: DUMMY_SP, // TODO
                    props: children_slots,
                }))
            } else {
                null!()
            };
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

    fn generate_component_children(&mut self, component_node: &ElementNode) -> Vec<PropOrSpread> {
        let mut result = Vec::new();
        let total_children = component_node.children.len();

        // No children work, return immediately
        if total_children == 0 {
            return result;
        }

        // Prepare the necessities.
        let component_span = DUMMY_SP; // todo
        let mut default_slot_children: Vec<Expr> = Vec::new();
        let with_ctx_ident = self.get_and_add_import_ident(VueImports::WithCtx);

        // `SlottedIterator` will iterate over sequences of default or named slots,
        // and it will stop yielding elements unless [`SlottedIterator::toggle_mode`] is called.
        let mut slotted_iterator = SlottedIterator::new(&component_node.children);

        // Generate the default slot items into the `default_slot_children` vec,
        // and named slots into the `result` vec.
        while slotted_iterator.has_more() {
            if slotted_iterator.is_default_slot_mode() {
                self.generate_node_sequence(
                    &mut slotted_iterator,
                    &mut default_slot_children,
                    total_children,
                    false,
                );
            } else {
                todo!("Named slots compilation")
            }

            slotted_iterator.toggle_mode();
        }

        // Add default slot children when needed
        if default_slot_children.len() != 0 {
            // e.g. child1, child2, child3
            let children_elems = default_slot_children
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
                span: component_span,
                elems: children_elems,
            };

            // withCtx(() => [child1, child2, child3])
            result.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: swc_core::ecma::ast::PropName::Ident(Ident {
                    span: component_span,
                    sym: js_word!("default"),
                    optional: false,
                }),
                value: Box::new(Expr::Call(CallExpr {
                    span: component_span,
                    // withCtx
                    callee: Callee::Expr(Box::new(Expr::Ident(Ident {
                        span: component_span,
                        sym: with_ctx_ident.clone(),
                        optional: false,
                    }))),
                    args: vec![ExprOrSpread {
                        spread: None,
                        // () => [child1, child2, child3]
                        expr: Box::new(Expr::Arrow(ArrowExpr {
                            span: component_span,
                            params: vec![],
                            body: Box::new(BlockStmtOrExpr::Expr(Box::new(Expr::Array(
                                children_arr,
                            )))),
                            is_async: false,
                            is_generator: false,
                            type_params: None,
                            return_type: None,
                        })),
                    }],
                    type_args: None,
                })),
            }))));
        }

        result
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
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fervid_core::{HtmlAttribute, Node, StartingTag};
    use swc_core::common::SourceMap;
    use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node as _};

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
        assert_eq!(to_str(out), expected)
    }

    fn to_str(expr: Expr) -> String {
        // Emitting the result requires some setup with SWC
        let cm: Arc<SourceMap> = Default::default();
        let mut buff: Vec<u8> = Vec::with_capacity(128);
        let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config {
                target: Default::default(),
                ascii_only: false,
                minify: true,
                omit_last_semi: false,
            },
            comments: None,
            wr: writer,
            cm,
        };

        let _ = expr.emit_with(&mut emitter);

        String::from_utf8(buff).unwrap()
    }
}
