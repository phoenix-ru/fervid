use fervid_core::{AttributeOrBinding, ElementKind, Node};
use swc_core::common::{Span, Spanned};

use crate::{
    TransformSfcContext,
    error::{TemplateError, TemplateErrorKind, TransformError},
    template::utils::is_static_arg_of,
};

/// Reports attributes and directives discarded with structural template carriers.
/// Note: this takes children and not a `Node` due to the timing of `transform_if`
/// that operates in `pre_transform_children`, i.e. before the individual Node
/// is visited. It matters because `transform_if` will actively discard some template carriers,
/// and we want to check them to alert the user of discarded attributes/directives.
/// See https://vuejs.org/api/built-in-special-elements.html#template
pub fn transform_template_attributes(ctx: &mut TransformSfcContext, children: &[Node]) {
    for child in children {
        let Node::Element(element) = child else {
            continue;
        };

        if !matches!(element.tag_type, ElementKind::Template) {
            continue;
        }

        let Some(directives) = element.starting_tag.directives.as_deref() else {
            continue;
        };

        // Templates which have none of the target directives are simply native elements.
        // `<template v-for>` is a template carrier;
        // `<template v-custom>` is a native element with a custom directive.
        let has_v_for = directives.v_for.is_some();
        if !has_v_for
            && directives.v_if.is_none()
            && directives.v_else_if.is_none()
            && directives.v_else.is_none()
            && directives.v_slot.is_none()
        {
            continue;
        }

        for attribute in &element.starting_tag.attributes {
            let is_v_for_key = has_v_for
                && match attribute {
                    AttributeOrBinding::RegularAttribute { name, .. } => name == "key",
                    AttributeOrBinding::VBind(binding) => {
                        is_static_arg_of(binding.argument.as_ref(), "key")
                    }
                    AttributeOrBinding::VOn(_) => false,
                };

            if !is_v_for_key {
                report_discarded_attribute(ctx, attribute.span());
            }
        }

        for expr in [
            directives.v_html.as_deref(),
            directives.v_show.as_deref(),
            directives.v_text.as_deref(),
            // `v-memo` is not discarded if paired with `v-for`
            if has_v_for {
                None
            } else {
                directives.v_memo.as_deref()
            },
        ]
        .into_iter()
        .flatten()
        {
            report_discarded_attribute(ctx, expr.span());
        }

        for directive in &directives.v_model {
            report_discarded_attribute(ctx, directive.span);
        }
        for directive in &directives.custom {
            report_discarded_attribute(ctx, directive.span);
        }
    }
}

fn report_discarded_attribute(ctx: &mut TransformSfcContext, span: Span) {
    ctx.errors
        .push(TransformError::TemplateError(TemplateError {
            span,
            kind: TemplateErrorKind::TemplateDiscardedAttribute,
        }));
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        AttributeOrBinding, ElementKind, ForParseResult, Node, VBindDirective, VCustomDirective,
        VForDirective, VModelDirective, VOnDirective, VueDirectives,
    };
    #[cfg(feature = "new-pipeline")]
    use fervid_core::{SfcTemplateBlock, VSlotDirective};
    use swc_core::common::{BytePos, DUMMY_SP, Span};

    use crate::test_utils::{element_from_tag, js};

    use super::*;

    // Note: The following test suite is Fervid-only.
    // vuejs-core does not report discarded attributes, but the discard behavior is mentioned
    // in the docs: https://vuejs.org/api/built-in-special-elements.html#template

    // Keys on <template> without v-for are discarded
    #[test]
    fn template_v_if_key_reports_ignored_key_for_single_and_multiple_children() {
        // <template v-if="ok" key="outer"><div/></template>
        // <template v-if="ok" key="outer"><div/><p/></template>
        // <div v-if="ok"/><template v-else key="outer"><p/></template>
        for roots in [
            vec![template_branch(
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![regular_key("outer")],
                vec![element("div")],
            )],
            vec![template_branch(
                VueDirectives {
                    v_if: Some(js("ok")),
                    ..Default::default()
                },
                vec![regular_key("outer")],
                vec![element("div"), element("p")],
            )],
            vec![
                element_with_if("div", "ok"),
                template_branch(
                    VueDirectives {
                        v_else: Some(()),
                        ..Default::default()
                    },
                    vec![regular_key("outer")],
                    vec![element("p")],
                ),
            ],
        ] {
            assert_diagnostic_count(&roots, 1);
        }
    }

    // Outer :key is ignored even when the rendered child already has a key
    #[test]
    fn template_v_if_bound_key_reports_ignored_key() {
        // <template v-if="ok" :key="outer"><div key="inner"/></template>
        let key_span = Span {
            lo: BytePos(15),
            hi: BytePos(27),
        };
        let mut key = bound_key("outer");
        let AttributeOrBinding::VBind(binding) = &mut key else {
            panic!("bound key fixture should contain v-bind");
        };
        binding.span = key_span;
        let roots = vec![template_branch(
            VueDirectives {
                v_if: Some(js("ok")),
                ..Default::default()
            },
            vec![key],
            vec![with_attributes(element("div"), vec![regular_key("inner")])],
        )];
        let mut ctx = TransformSfcContext::anonymous();
        transform_template_attributes(&mut ctx, &roots);

        let [TransformError::TemplateError(error)] = ctx.errors.as_slice() else {
            panic!(
                "expected one discarded template attribute diagnostic: {:#?}",
                ctx.errors
            );
        };
        assert!(matches!(
            error.kind,
            TemplateErrorKind::TemplateDiscardedAttribute
        ));
        assert_eq!(error.span, key_span);
    }

    // Structural template attributes are discarded
    #[test]
    fn template_v_if_discards_regular_bound_and_event_attributes() {
        // <template v-if="ok" id="ignored"><div/></template>
        // <template v-if="ok" :title="title"><div/></template>
        // <template v-if="ok" @click="onClick"><div/></template>
        for attribute in [
            regular_attribute("id", "ignored"),
            AttributeOrBinding::VBind(VBindDirective {
                argument: Some("title".into()),
                value: js("title"),
                is_camel: false,
                is_prop: false,
                is_attr: false,
                span: DUMMY_SP,
            }),
            AttributeOrBinding::VOn(VOnDirective {
                event: Some("click".into()),
                handler: Some(js("onClick")),
                modifiers: vec![],
                span: DUMMY_SP,
            }),
        ] {
            assert_diagnostic_count(
                &[template_branch(
                    VueDirectives {
                        v_if: Some(js("ok")),
                        ..Default::default()
                    },
                    vec![attribute],
                    vec![element("div")],
                )],
                1,
            );
        }
    }

    // Runtime directives on structural templates have no matching element
    #[test]
    fn template_v_if_discards_v_show() {
        // <template v-if="ok" v-show="visible"><div/></template>
        assert_diagnostic_count(
            &[template_branch(
                VueDirectives {
                    v_if: Some(js("ok")),
                    v_show: Some(js("visible")),
                    ..Default::default()
                },
                vec![],
                vec![element("div")],
            )],
            1,
        );
    }

    // v-html, v-text, custom directives and v-model are also discarded
    #[test]
    fn template_v_if_discards_other_runtime_directives() {
        // <template v-if="ok" v-html="html"><div/></template>
        // <template v-if="ok" v-text="text"><div/></template>
        // <template v-if="ok" v-foo><div/></template>
        // <template v-if="ok" v-model="value"><div/></template>
        let cases = [
            VueDirectives {
                v_html: Some(js("html")),
                ..Default::default()
            },
            VueDirectives {
                v_text: Some(js("text")),
                ..Default::default()
            },
            VueDirectives {
                custom: vec![VCustomDirective {
                    name: "foo".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            VueDirectives {
                v_model: vec![VModelDirective {
                    argument: None,
                    value: js("value"),
                    update_handler: None,
                    modifiers: vec![],
                    span: DUMMY_SP,
                }],
                ..Default::default()
            },
        ];
        let v_if = Some(js("ok"));
        for mut directives in cases {
            directives.v_if = v_if.clone();
            assert_diagnostic_count(
                &[template_branch(directives, vec![], vec![element("div")])],
                1,
            );
        }
    }

    // v-once still affects the conditional and v-memo is supported on v-for
    // Vue reference: https://github.com/vuejs/core/blob/68f25a4a1957710de71731702bd42e05535f494b/packages/compiler-core/__tests__/transforms/vOnce.spec.ts#L109-L132
    #[test]
    fn meaningful_template_directives_do_not_report_error() {
        // <template v-if="ok" v-once><div/></template>
        // <template v-for="item in items" v-memo="[item.id]"><div/></template>
        assert_diagnostic_count(
            &[
                template_branch(
                    VueDirectives {
                        v_if: Some(js("ok")),
                        v_once: Some(()),
                        ..Default::default()
                    },
                    vec![],
                    vec![element("div")],
                ),
                template_branch(
                    VueDirectives {
                        v_for: Some(v_for_directive()),
                        v_memo: Some(js("[item.id]")),
                        ..Default::default()
                    },
                    vec![],
                    vec![element("div")],
                ),
            ],
            0,
        );
    }

    // Plain <template> remains a native element and its attributes are meaningful
    #[test]
    fn native_template_attributes_do_not_report_discarded_attribute() {
        // <template><template id="kept" key="native"><div/></template></template>
        let mut native_template = element_from_tag("template");
        native_template.starting_tag.attributes =
            vec![regular_attribute("id", "kept"), regular_key("native")];
        native_template.children.push(element("div"));
        assert_diagnostic_count(&[Node::Element(native_template)], 0);
    }

    // The exception for a v-for key does not preserve other template attributes
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn template_v_for_key_does_not_preserve_other_attributes() {
        // <template v-for="item in items" :key="item.id" id="ignored"><div/></template>
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![template_branch(
                VueDirectives {
                    v_for: Some(v_for_directive()),
                    ..Default::default()
                },
                vec![bound_key("item.id"), regular_attribute("id", "ignored")],
                vec![element("div")],
            )],
            span: DUMMY_SP,
        };
        let mut ctx = TransformSfcContext::anonymous();
        crate::template::transform_and_record_template(&mut template, &mut ctx);
        assert_discarded_errors(&ctx, 1);
    }

    // v-slot carrier does not render its ordinary attributes either
    #[cfg(feature = "new-pipeline")]
    #[test]
    fn template_v_slot_discards_regular_attributes() {
        // <Comp><template v-slot:default id="ignored"><div/></template></Comp>
        let mut component = element_from_tag("Comp");
        component.tag_type = ElementKind::Component;
        component.children.push(template_branch(
            VueDirectives {
                v_slot: Some(VSlotDirective {
                    slot_name: Some("default".into()),
                    value: None,
                }),
                ..Default::default()
            },
            vec![regular_attribute("id", "ignored")],
            vec![element("div")],
        ));
        let mut template = SfcTemplateBlock {
            lang: "html".into(),
            roots: vec![Node::Element(component)],
            span: DUMMY_SP,
        };
        let mut ctx = TransformSfcContext::anonymous();
        crate::template::transform_and_record_template(&mut template, &mut ctx);
        assert_discarded_errors(&ctx, 1);
    }

    // Template v-for key is meaningful even beside v-if
    // Vue docs: https://vuejs.org/guide/essentials/list.html#maintaining-state-with-key
    #[test]
    fn template_v_if_v_for_key_does_not_report_ignored_key() {
        // <template v-if="ok" v-for="item in items" :key="item.id"><div/></template>
        // <template v-if="ok" v-for="item in items" :key="item.id" id="ignored"><div/></template>
        for (attributes, expected_count) in [
            (vec![bound_key("item.id")], 0),
            (
                vec![bound_key("item.id"), regular_attribute("id", "ignored")],
                1,
            ),
        ] {
            assert_diagnostic_count(
                &[template_branch(
                    VueDirectives {
                        v_if: Some(js("ok")),
                        v_for: Some(v_for_directive()),
                        ..Default::default()
                    },
                    attributes,
                    vec![element("div")],
                )],
                expected_count,
            );
        }
    }

    fn assert_diagnostic_count(children: &[Node], expected: usize) {
        let mut ctx = TransformSfcContext::anonymous();
        transform_template_attributes(&mut ctx, children);
        assert_discarded_errors(&ctx, expected);
    }

    fn assert_discarded_errors(ctx: &TransformSfcContext, expected: usize) {
        assert_eq!(ctx.errors.len(), expected, "{:#?}", ctx.errors);
        assert!(
            ctx.errors.iter().all(|error| matches!(error,
                TransformError::TemplateError(error)
                    if matches!(error.kind, TemplateErrorKind::TemplateDiscardedAttribute)
            )),
            "{:#?}",
            ctx.errors
        );
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

    fn element(tag: &str) -> Node {
        Node::Element(element_from_tag(tag))
    }

    fn element_with_if(tag: &str, condition: &str) -> Node {
        let mut element = element_from_tag(tag);
        element.starting_tag.directives = Some(Box::new(VueDirectives {
            v_if: Some(js(condition)),
            ..Default::default()
        }));
        Node::Element(element)
    }

    fn with_attributes(mut node: Node, attributes: Vec<AttributeOrBinding>) -> Node {
        let Node::Element(element) = &mut node else {
            panic!("test fixture should contain an element");
        };
        element.starting_tag.attributes = attributes;
        node
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

    fn v_for_directive() -> VForDirective {
        VForDirective {
            parse_result: Box::new(ForParseResult {
                source: js("items"),
                value: js("item"),
                key: None,
                index: None,
                finalized: false,
                finalized_is_dynamic: false,
            }),
            patch_flags: Default::default(),
            span: DUMMY_SP,
        }
    }
}
