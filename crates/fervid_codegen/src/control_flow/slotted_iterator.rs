use fervid_core::{is_from_default_slot, Node};

#[derive(PartialEq)]
pub enum SlottedIteratorMode {
    // Iterating over the default slot
    Default,
    // Iterating over the named slot
    Named,
}

pub struct SlottedIterator<'n> {
    nodes: &'n [Node],
    idx: usize,
    mode: SlottedIteratorMode,
}

impl<'n> Iterator for SlottedIterator<'n> {
    type Item = &'n Node;

    /// Gets the next item and advances the iterator
    fn next(&mut self) -> Option<Self::Item> {
        let next_item = self.peek();
        if next_item.is_some() {
            self.idx += 1;
        }
        next_item
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.nodes.len(), Some(self.nodes.len()))
    }
}

impl<'n> SlottedIterator<'n> {
    pub fn new(nodes: &'n [Node]) -> Self {
        SlottedIterator {
            nodes,
            idx: 0,
            mode: SlottedIteratorMode::Default,
        }
    }

    /// Switches from iterating over default slot to named, and back
    pub fn toggle_mode(&mut self) {
        self.mode = if self.is_default_slot_mode() {
            SlottedIteratorMode::Named
        } else {
            SlottedIteratorMode::Default
        };
    }

    /// Switches the iteration mode if peek() yields None
    #[inline]
    pub fn toggle_mode_if_peek_is_none(&mut self) {
        if self.peek().is_none() {
            self.toggle_mode();
        }
    }

    /// Is iteration mode Default
    #[inline]
    pub fn is_default_slot_mode(&self) -> bool {
        self.mode == SlottedIteratorMode::Default
    }

    /// Whether there are more elements to consume, irrespective of mode
    #[inline]
    pub fn has_more(&self) -> bool {
        self.idx < self.nodes.len()
    }

    /// Custom peek implementation.
    /// Gets the next item from the same slot as current iteration mode,
    /// but does not advance the iterator.
    ///
    /// To switch mode, use [`SlottedIterator::toggle_mode`]
    pub fn peek(&self) -> Option<&'n Node> {
        match self.nodes.get(self.idx) {
            Some(node) => {
                // From default slot and mode is Default,
                // or not from default slot and mode is Named
                let is_suitable =
                    (self.mode == SlottedIteratorMode::Default) == is_from_default_slot(node);

                if is_suitable {
                    Some(node)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Not a safe method, please avoid it in favor of `next`
    /// This is only made to work in tandem with [`SlottedIterator::peek`]
    #[inline]
    #[allow(unused)]
    pub fn advance(&mut self) {
        self.idx += 1;
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        AttributeOrBinding, ElementKind, ElementNode, StartingTag, VOnDirective, VSlotDirective,
        VueDirectives,
    };
    use swc_core::common::DUMMY_SP;

    use crate::test_utils::{js, regular_attribute, v_bind_attribute};

    use super::*;

    #[test]
    fn it_returns_all_default() {
        let items = vec![
            get_default_item1(),
            get_default_item2(),
            get_default_item3(),
            get_default_item4(),
            get_default_item5(),
        ];

        let iter = SlottedIterator::new(&items);
        assert_eq!(5, iter.count());
    }

    #[test]
    fn it_doesnot_proceed_when_named() {
        let items = vec![
            get_named_item1(),
            get_default_item1(),
            get_default_item2(),
            get_default_item3(),
            get_default_item4(),
            get_default_item5(),
        ];

        let mut iter = SlottedIterator::new(&items);

        // Ensure that calling iterator over and over yields the same result
        for _ in 0..100 {
            assert!(iter.next().is_none());
        }
    }

    #[test]
    fn it_consumes_one_when_named_mode() {
        let items = vec![
            get_named_item1(),
            get_default_item1(),
            get_default_item2(),
            get_default_item3(),
            get_default_item4(),
            get_default_item5(),
        ];

        let mut iter = SlottedIterator::new(&items);
        iter.toggle_mode();

        assert!(iter.next().is_some());
        assert!(iter.next().is_none());
    }

    #[test]
    fn it_consumes_all_in_mixed_operation() {
        // 6 default slot items, 4 named slot items
        let items = vec![
            get_default_item1(),
            get_named_item1(),
            get_default_item2(),
            get_default_item3(),
            get_named_item1(),
            get_default_item4(),
            get_default_item5(),
            get_named_item1(),
            get_named_item1(),
            get_default_item1(),
        ];

        let mut iter = SlottedIterator::new(&items);

        let mut cnt = [0, 0]; // [default, named] counts
        let mut curr_incr = 0;

        // Just count default and named
        while iter.has_more() {
            for _ in iter.by_ref() {
                cnt[curr_incr] += 1;
            }

            // 0 -> 1, 1 -> 0
            curr_incr = (curr_incr + 1) % 2;
            iter.toggle_mode();
        }

        assert_eq!(cnt, [6, 4]);
    }

    /// <h1>This is an h1</h1>
    fn get_default_item1() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h1".into(),
                attributes: vec![],
                directives: None,
            },
            children: vec![Node::Text("This is an h1".into(), DUMMY_SP)],
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
        })
    }

    /// <div class="regular" :disabled="true" />
    fn get_default_item2() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "div".into(),
                attributes: vec![
                    regular_attribute("class", "regular"),
                    v_bind_attribute("disabled", "true"),
                ],
                directives: None,
            },
            children: vec![],
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
        })
    }

    /// <test-component :foo="bar" @event="baz">This is a component</test-component>
    fn get_default_item3() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h1".into(),
                attributes: vec![
                    v_bind_attribute("disabled", "true"),
                    AttributeOrBinding::VOn(VOnDirective {
                        event: Some("event".into()),
                        handler: Some(js("baz")),
                        modifiers: vec![],
                        span: DUMMY_SP,
                    }),
                ],
                directives: None,
            },
            children: vec![Node::Text("This is a component".into(), DUMMY_SP)],
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
        })
    }

    /// <template>This is just a template</template>
    fn get_default_item4() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: None,
            },
            children: vec![Node::Text("This is just a template".into(), DUMMY_SP)],
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
        })
    }

    /// <template v-slot:default>This is a default template</template>
    fn get_default_item5() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_slot: Some(VSlotDirective {
                        slot_name: Some("default".into()),
                        value: None,
                    }),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("This is a default template".into(), DUMMY_SP)],
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
        })
    }

    /// <template v-slot:named>This is a default template</template>
    fn get_named_item1() -> Node {
        Node::Element(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template".into(),
                attributes: vec![],
                directives: Some(Box::new(VueDirectives {
                    v_slot: Some(VSlotDirective {
                        slot_name: Some("named".into()),
                        value: None,
                    }),
                    ..Default::default()
                })),
            },
            children: vec![Node::Text("This is a named template".into(), DUMMY_SP)],
            template_scope: 0,
            kind: ElementKind::Element,
            patch_hints: Default::default(),
            span: DUMMY_SP,
        })
    }
}
