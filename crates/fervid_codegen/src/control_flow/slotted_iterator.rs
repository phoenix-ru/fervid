use fervid_core::{ElementNode, HtmlAttribute, Node, VDirective};

#[derive(PartialEq)]
pub enum SlottedIteratorMode {
    // Iterating over the default slot
    Default,
    // Iterating over the named slot
    Named,
}

pub struct SlottedIterator<'n> {
    nodes: &'n [Node<'n>],
    idx: usize,
    mode: SlottedIteratorMode,
}

impl<'n> Iterator for SlottedIterator<'n> {
    type Item = &'n Node<'n>;

    /// Gets the next item and advances the iterator
    fn next(&mut self) -> Option<Self::Item> {
        let next_item = self.peek();
        if let Some(_) = next_item {
            self.idx += 1;
        }
        next_item
    }

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

    /// Is iteration mode Default
    pub fn is_default_slot_mode(&self) -> bool {
        self.mode == SlottedIteratorMode::Default
    }

    /// Whether there are more elements to consume, irrespective of mode
    pub fn has_more(&self) -> bool {
        self.idx < self.nodes.len()
    }

    /// Custom peek implementation.
    /// Gets the next item from the same slot as current iteration mode,
    /// but does not advance the iterator.
    ///
    /// To switch mode, use [`SlottedIterator::toggle_mode`]
    pub fn peek(&self) -> Option<&'n Node<'n>> {
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
            None => None
        }
    }

    /// Not a safe method, please avoid it in favor of `next`
    /// This is only made to work in tandem with [`SlottedIterator::peek`]
    pub fn advance(&mut self) {
        self.idx += 1;
    }
}

fn is_from_default_slot(node: &Node) -> bool {
    match node {
        Node::ElementNode(ElementNode { starting_tag, .. }) => {
            if starting_tag.tag_name != "template" {
                return true;
            }

            // Slot is not default if its `v-slot` has an argument which is not "" or "default"
            // `v-slot` is default
            // `v-slot:default` is default
            // `v-slot:custom` is not default
            // `v-slot:[default]` is not default
            !starting_tag.attributes.iter().any(|attr| match attr {
                HtmlAttribute::VDirective(VDirective::Slot(v_slot)) => {
                    v_slot.is_dynamic_slot
                        || match v_slot.slot_name {
                            None | Some("default") => false,
                            Some(_) => true,
                        }
                }

                _ => false,
            })
        }

        // explicit just in case I decide to change node types and forget about this place
        Node::DynamicExpression { .. } | Node::TextNode(_) | Node::CommentNode(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{StartingTag, VBindDirective, VOnDirective, VSlotDirective};

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
            while let Some(_) = iter.next() {
                cnt[curr_incr] += 1;
            }

            // 0 -> 1, 1 -> 0
            curr_incr = (curr_incr + 1) % 2;
            iter.toggle_mode();
        }

        assert_eq!(cnt, [6, 4]);
    }

    /// <h1>This is an h1</h1>
    fn get_default_item1() -> Node<'static> {
        Node::ElementNode(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h1",
                attributes: vec![],
                is_self_closing: false,
                kind: fervid_core::ElementKind::Normal,
            },
            children: vec![Node::TextNode("This is an h1")],
            template_scope: 0,
        })
    }

    /// <div class="regular" :disabled="true" />
    fn get_default_item2() -> Node<'static> {
        Node::ElementNode(ElementNode {
            starting_tag: StartingTag {
                tag_name: "div",
                attributes: vec![
                    HtmlAttribute::Regular {
                        name: "class",
                        value: "regular",
                    },
                    HtmlAttribute::VDirective(VDirective::Bind(fervid_core::VBindDirective {
                        argument: Some("disabled"),
                        value: "true",
                        is_dynamic_attr: false,
                        is_camel: false,
                        is_prop: false,
                        is_attr: false,
                    })),
                ],
                is_self_closing: true,
                kind: fervid_core::ElementKind::Normal,
            },
            children: vec![],
            template_scope: 0,
        })
    }

    /// <test-component :foo="bar" @event="baz">This is a component</test-component>
    fn get_default_item3() -> Node<'static> {
        Node::ElementNode(ElementNode {
            starting_tag: StartingTag {
                tag_name: "h1",
                attributes: vec![
                    HtmlAttribute::VDirective(VDirective::Bind(VBindDirective {
                        argument: Some("disabled"),
                        value: "true",
                        is_dynamic_attr: false,
                        is_camel: false,
                        is_prop: false,
                        is_attr: false,
                    })),
                    HtmlAttribute::VDirective(VDirective::On(VOnDirective {
                        event: Some("event"),
                        handler: Some("baz"),
                        is_dynamic_event: false,
                        modifiers: vec![],
                    })),
                ],
                is_self_closing: true,
                kind: fervid_core::ElementKind::Normal,
            },
            children: vec![Node::TextNode("This is a component")],
            template_scope: 0,
        })
    }

    /// <template>This is just a template</template>
    fn get_default_item4() -> Node<'static> {
        Node::ElementNode(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template",
                attributes: vec![],
                is_self_closing: false,
                kind: fervid_core::ElementKind::Normal,
            },
            children: vec![Node::TextNode("This is just a template")],
            template_scope: 0,
        })
    }

    /// <template v-slot:default>This is a default template</template>
    fn get_default_item5() -> Node<'static> {
        Node::ElementNode(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template",
                attributes: vec![HtmlAttribute::VDirective(VDirective::Slot(
                    VSlotDirective {
                        slot_name: Some("default"),
                        value: None,
                        is_dynamic_slot: false,
                    },
                ))],
                is_self_closing: false,
                kind: fervid_core::ElementKind::Normal,
            },
            children: vec![Node::TextNode("This is a default template")],
            template_scope: 0,
        })
    }

    /// <template v-slot:named>This is a default template</template>
    fn get_named_item1() -> Node<'static> {
        Node::ElementNode(ElementNode {
            starting_tag: StartingTag {
                tag_name: "template",
                attributes: vec![HtmlAttribute::VDirective(VDirective::Slot(
                    VSlotDirective {
                        slot_name: Some("named"),
                        value: None,
                        is_dynamic_slot: false,
                    },
                ))],
                is_self_closing: false,
                kind: fervid_core::ElementKind::Normal,
            },
            children: vec![Node::TextNode("This is a named template")],
            template_scope: 0,
        })
    }
}
