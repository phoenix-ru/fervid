use swc_core::common::{Span, Spanned, DUMMY_SP};
use swc_css_ast::{
    AtRule, AttributeSelector, Combinator, ComplexSelector, ComplexSelectorChildren,
    ComponentValue, Ident, ListOfComponentValues, PseudoClassSelectorChildren,
    PseudoElementSelectorChildren, QualifiedRulePrelude, Rule, SelectorList, SimpleBlock,
    Stylesheet, SubclassSelector, WqName,
};

use super::{
    codegen::{
        stringify_pseudo_class_selector_children, stringify_pseudo_element_selector_children,
    },
    error::CssError,
    parse::parse_complex_selector,
};

pub struct ScopedTransformer<'s> {
    scope: &'s str,
    errors: Vec<CssError>,
}

impl<'s> ScopedTransformer<'s> {
    pub fn new(scope: &'s str) -> Self {
        Self {
            scope,
            errors: vec![],
        }
    }

    pub fn transform(&mut self, stylesheet: &mut Stylesheet) {
        for rule in stylesheet.rules.iter_mut() {
            match rule {
                Rule::QualifiedRule(qualified_rule) => match qualified_rule.prelude {
                    QualifiedRulePrelude::SelectorList(ref mut selector_list) => {
                        self.transform_selector_list(selector_list);
                    }
                    QualifiedRulePrelude::RelativeSelectorList(_) => {}
                    QualifiedRulePrelude::ListOfComponentValues(
                        ref mut list_of_component_values,
                    ) => {
                        self.transform_list_of_component_values(list_of_component_values);
                    }
                },

                Rule::AtRule(at_rule) => {
                    self.transform_at_rule(at_rule);
                }

                Rule::ListOfComponentValues(list_of_component_values) => {
                    self.transform_list_of_component_values(list_of_component_values);
                }
            }
        }
    }

    pub fn take_errors(&mut self) -> Vec<CssError> {
        std::mem::take(&mut self.errors)
    }

    /// This is the meat of the scoped transform
    pub fn transform_complex_selector(&mut self, complex_selector: &mut ComplexSelector) {
        let mut deep_idx: Option<usize> = None;
        let mut deep_children: Option<ComplexSelector> = None;
        let mut is_deep_alone = false;
        let mut previous_compound_selector_idx = 0;

        // 1.
        // Search phase. This would find the `:deep` or `::v-deep`,
        // take its contents and also rewrite when it is not alone in `CompoundSelector`
        for (idx, complex_selector_child) in complex_selector.children.iter_mut().enumerate() {
            let ComplexSelectorChildren::CompoundSelector(compound_selector) =
                complex_selector_child
            else {
                continue;
            };

            let compound_selector_len = compound_selector.subclass_selectors.len()
                + compound_selector.type_selector.is_some() as usize // 1 if Some
                + compound_selector.nesting_selector.is_some() as usize; // 1 if Some

            // Find the `:deep`
            let deep = compound_selector
                .subclass_selectors
                .iter_mut()
                .find(|sel| match sel {
                    SubclassSelector::PseudoClass(pseudo) if pseudo.name.value == "deep" => true,
                    SubclassSelector::PseudoElement(pseudo) if pseudo.name.value == "v-deep" => {
                        true
                    }
                    _ => false,
                });

            // Remember the `CompoundSelector` idx if no `:deep` or it is not alone.
            // This is for correctly inserting the `AttributeSelector`.
            if deep.is_none() || compound_selector_len != 1 {
                previous_compound_selector_idx = idx;
            }

            let Some(deep) = deep else {
                continue;
            };

            // Alone means there are no other selectors in this `CompoundSelector`
            is_deep_alone = compound_selector_len == 1;

            // Take `children` from `:deep` or `::v-deep`
            match deep {
                SubclassSelector::PseudoClass(deep_pseudo_class) => {
                    if let Some(children) = deep_pseudo_class.children.take() {
                        deep_children = process_pseudo_class_children(children, &mut self.errors);
                    }
                }

                SubclassSelector::PseudoElement(deep_pseudo_element) => {
                    if let Some(children) = deep_pseudo_element.children.take() {
                        deep_children = process_pseudo_element_children(children, &mut self.errors);
                    }
                }

                _ => unreachable!(),
            }

            // Rewrite deep with `AttributeSelector` (e.g. `[data-v-abcd]`)
            *deep = self.get_subclass_selector_to_add();

            deep_idx = Some(idx);
            break;
        }

        // 2.
        // Check that we actually found `:deep`.
        // If not, just add to the `previous_compound_selector_idx`
        let Some(deep_idx) = deep_idx else {
            if let Some(ComplexSelectorChildren::CompoundSelector(last_compound_selector)) =
                complex_selector
                    .children
                    .get_mut(previous_compound_selector_idx)
            {
                last_compound_selector
                    .subclass_selectors
                    .push(self.get_subclass_selector_to_add());
            }

            return;
        };

        // Special case: deep is the only selector in `ComplexSelector`
        let is_deep_really_alone = complex_selector.children.len() == 1;

        // 3.
        // Cut the array after the `deep_idx`
        let mut selectors_after_deep: Vec<ComplexSelectorChildren> =
            complex_selector.children.drain((deep_idx + 1)..).collect();

        // 4.
        // Check if deep we found is alone.
        // In this case we need to add `[data-v]` to the previous `CompoundSelector`.
        if is_deep_alone && !is_deep_really_alone {
            // Remove the lonely `:deep` or `::v-deep` (which was already transformed to `[data-v]`)
            let Some(ComplexSelectorChildren::CompoundSelector(mut deep_alone)) =
                complex_selector.children.pop()
            else {
                unreachable!()
            };

            let Some(subclass_selector) = deep_alone.subclass_selectors.pop() else {
                unreachable!()
            };

            // Add to the previously found `CompoundSelector`
            let Some(ComplexSelectorChildren::CompoundSelector(previous_compound_selector)) =
                complex_selector
                    .children
                    .get_mut(previous_compound_selector_idx)
            else {
                unreachable!()
            };

            previous_compound_selector
                .subclass_selectors
                .push(subclass_selector);
        } else if deep_children.is_some() && (is_deep_really_alone || !is_deep_alone) {
            // Add descendant Combinator (` `) when deep is:
            // - part of other `CompoundSelector`, or
            // - a single selector inside `ComplexSelector` and has children.
            complex_selector
                .children
                .push(ComplexSelectorChildren::Combinator(Combinator {
                    span: DUMMY_SP,
                    value: swc_css_ast::CombinatorValue::Descendant,
                }));
        }

        // 5.
        // Add children of deep
        if let Some(mut deep_children_parsed) = deep_children {
            complex_selector
                .children
                .append(&mut deep_children_parsed.children);
        }

        // 6.
        // Put back the remaining parts
        complex_selector.children.append(&mut selectors_after_deep);
    }

    /// 0. Prepare what selector to add.
    ///    It is always an attribute selector, e.g. `[data-v-abcd1234]`
    fn get_subclass_selector_to_add(&self) -> SubclassSelector {
        SubclassSelector::Attribute(Box::new(AttributeSelector {
            span: DUMMY_SP,
            name: WqName {
                span: DUMMY_SP,
                prefix: None,
                value: Ident {
                    span: DUMMY_SP,
                    value: self.scope.into(),
                    raw: None,
                },
            },
            matcher: None,
            value: None,
            modifier: None,
        }))
    }

    fn transform_at_rule(&mut self, at_rule: &mut AtRule) {
        if let Some(ref mut at_rule_block) = at_rule.block {
            self.transform_simple_block(at_rule_block);
        };
    }

    fn transform_component_value(&mut self, component_value: &mut ComponentValue) {
        match component_value {
            ComponentValue::QualifiedRule(qual) => match qual.prelude {
                QualifiedRulePrelude::SelectorList(ref mut selector_list) => {
                    self.transform_selector_list(selector_list);
                }
                QualifiedRulePrelude::RelativeSelectorList(_) => {}
                QualifiedRulePrelude::ListOfComponentValues(ref mut list_of_component_values) => {
                    self.transform_list_of_component_values(list_of_component_values);
                }
            },

            ComponentValue::ComplexSelector(complex_selector) => {
                self.transform_complex_selector(complex_selector);
            }

            ComponentValue::AtRule(at_rule) => {
                self.transform_at_rule(at_rule);
            }

            ComponentValue::SimpleBlock(simple_block) => {
                self.transform_simple_block(simple_block);
            }

            ComponentValue::ListOfComponentValues(list_of_component_values) => {
                self.transform_list_of_component_values(list_of_component_values);
            }

            _ => {}
        }
    }

    fn transform_list_of_component_values(
        &mut self,
        list_of_component_values: &mut ListOfComponentValues,
    ) {
        for component_value in list_of_component_values.children.iter_mut() {
            self.transform_component_value(component_value);
        }
    }

    fn transform_selector_list(&mut self, selector_list: &mut SelectorList) {
        for complex_selector in selector_list.children.iter_mut() {
            self.transform_complex_selector(complex_selector);
        }
    }

    fn transform_simple_block(&mut self, simple_block: &mut SimpleBlock) {
        for component_value in simple_block.value.iter_mut() {
            self.transform_component_value(component_value);
        }
    }
}

// Processes contents of `:deep`
fn process_pseudo_class_children(
    children: Vec<PseudoClassSelectorChildren>,
    errors: &mut Vec<CssError>,
) -> Option<ComplexSelector> {
    // Figure out the span
    let first_child = children.first()?;
    let last_child = children.last()?;
    let span = Span {
        lo: first_child.span_lo(),
        hi: last_child.span_hi(),
    };

    // Stringify
    let stringified = stringify_pseudo_class_selector_children(children);

    // Parse
    let mut parse_errors = Vec::new();
    let result = parse_complex_selector(&stringified, span, &mut parse_errors);

    // Take errors, assume they are recoverable
    errors.reserve(parse_errors.len());
    for error in parse_errors {
        errors.push(CssError::from_parse_error(error, true, true));
    }

    match result {
        Ok(complex_selector) => Some(complex_selector),
        Err(e) => {
            errors.push(CssError::from_parse_error(e, false, true));
            None
        }
    }
}

// Processes contents of `::v-deep`
fn process_pseudo_element_children(
    children: Vec<PseudoElementSelectorChildren>,
    errors: &mut Vec<CssError>,
) -> Option<ComplexSelector> {
    // Figure out the span
    let first_child = children.first()?;
    let last_child = children.last()?;
    let span = Span {
        lo: first_child.span_lo(),
        hi: last_child.span_hi(),
    };

    // Stringify
    let stringified = stringify_pseudo_element_selector_children(children);

    // Parse
    let mut parse_errors = Vec::new();
    let result = parse_complex_selector(&stringified, span, &mut parse_errors);

    // Take errors, assume they are recoverable
    errors.reserve(parse_errors.len());
    for error in parse_errors {
        errors.push(CssError::from_parse_error(error, true, true));
    }

    match result {
        Ok(complex_selector) => Some(complex_selector),
        Err(e) => {
            errors.push(CssError::from_parse_error(e, false, true));
            None
        }
    }
}
