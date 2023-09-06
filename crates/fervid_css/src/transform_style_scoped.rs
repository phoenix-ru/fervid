use cssparser::CowRcStr;
use lightningcss::{
    error::{Error, MinifyErrorKind, ParserError, PrinterErrorKind},
    printer::Printer,
    properties::custom::TokenOrValue,
    rules::CssRule,
    selector::{Component, PseudoClass, Selector},
    stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet, ToCssResult},
    traits::ParseWithOptions,
};
use parcel_selectors::parser::Combinator;

#[derive(Default)]
pub struct TransformOptions<'a> {
    pub parse: ParserOptions<'a, 'a>,
    pub minify: Option<MinifyOptions>,
    pub to_css: PrinterOptions<'a>,
}

#[derive(Debug)]
pub enum TransformError<'i> {
    ParserError(Error<ParserError<'i>>),
    MinifyError(Error<MinifyErrorKind>),
    PrinterError(Error<PrinterErrorKind>),
}

pub struct Transformer<'i> {
    input: &'i str,
    scope: &'i str,
    cache: Vec<String>,
}

impl<'i> Transformer<'i> {
    /// Creates a new <u>single-use</u> transformer tied to `input`.
    ///
    /// `scope` is the prefix which should be applied, e.g. `data-v-abcd1234`
    pub fn new(input: &'i str, scope: &'i str) -> Self {
        Self {
            input,
            scope,
            cache: vec![],
        }
    }

    /// Transforms stylesheets using `lightningcss`.
    /// After calling the method, transformer instance becomes unusable and any subsequent calls
    /// will trigger a panic.
    ///
    /// ### Why is transformer not consumed?
    /// Because `lightningcss` aims to be a borrow parser,
    /// dynamically rewriting `:deep()` rules is a tricky problem due to Rust's lifetimes.
    /// That is why we need to persuade the compiler that all the strings we create will be valid
    /// for at least the same lifetime as the input stylesheet, and this is what `cache` is for.
    ///
    /// An additional benefit of using `cache` is that we can reliably return warnings and errors
    /// produced by `lightningcss`, because they must have the same lifetime
    /// as input (and as transformer).
    pub fn transform_style_scoped(
        &'i mut self,
        options: TransformOptions<'i>,
    ) -> Result<ToCssResult, TransformError<'i>> {
        // Check if Transformer was consumed already
        // Because we always need to tie output and `self.cache` to `self.input`,
        // sadly we cannot just consume Transformer and have to use a reference
        if self.cache.len() > 0 {
            panic!("Transformer was already consumed")
        }

        let TransformOptions {
            parse,
            minify,
            to_css,
        } = options;

        let mut stylesheet = StyleSheet::parse(self.input, parse)?;

        let suffix = self.scope.into();

        transform_cached_strategy(&mut stylesheet, &suffix, &mut self.cache);

        if let Some(minify_options) = minify {
            stylesheet.minify(minify_options)?;
        }

        Ok(stylesheet.to_css(to_css)?)
    }
}

/// We cannot use `Visitor` from `lightningcss`, because it does not ensure
/// that `Visitor` has at least the same lifetime as its input.
/// And we need this guarantee to satisfy Rust's lifetime checks,
/// simply because we are creating new `String`s to parse and attach
/// the contents of `:deep()`.
///
/// Another method could have been using a static set,
/// but this sounds like even more effort, and potentially dangerous in WASM.
fn transform_cached_strategy<'i>(
    stylesheet: &mut StyleSheet<'i, '_>,
    suffix: &CowRcStr<'i>,
    cache: &'i mut Vec<String>,
) {
    // Collect phase, because we cannot write to cache and reference it at the same time
    // Both phases must be identical in items they visit, otherwise `cache` will get out-of-bounds
    for rule in stylesheet.rules.0.iter_mut() {
        let CssRule::Style(style) = rule else {
            continue;
        };

        for selector in style.selectors.0.iter_mut() {
            let mut iter = selector.iter_raw_match_order();

            while let Some(part) = iter.next() {
                let Component::NonTSPseudoClass(PseudoClass::CustomFunction { name, arguments }) = part else {
                    continue;
                };

                if *name != "deep" {
                    continue;
                }

                let mut deep_css = String::new();
                let mut printer = Printer::new(&mut deep_css, Default::default());

                for arg in arguments.0.iter() {
                    let TokenOrValue::Token(token) = arg else { continue; };
                    lightningcss::traits::ToCss::to_css(token, &mut printer).unwrap();
                }

                // Cache MUST be pushed to inside this loop,
                // otherwise referencing it later will panic
                cache.push(deep_css);
                break;
            }
        }
    }

    let mut ptr: usize = 0;

    macro_rules! to_append {
        () => {
            Component::AttributeInNoNamespaceExists {
                local_name: suffix.clone().into(),
                local_name_lower: suffix.clone().into(),
            }
        };
    }

    macro_rules! is_combinator {
        ($what: ident) => {
            matches!(
                $what,
                Component::Combinator(
                    Combinator::Child
                        | Combinator::Descendant
                        | Combinator::LaterSibling
                        | Combinator::NextSibling
                )
            )
        };
    }

    // Write phase, cached contents of `:deep`s will be parsed as selectors
    for rule in stylesheet.rules.0.iter_mut() {
        let CssRule::Style(style) = rule else {
            continue;
        };

        for selector in style.selectors.0.iter_mut() {
            let selector_len = selector.len();
            let iter = &mut selector.iter_mut_raw_match_order().enumerate();

            let mut components: Vec<Component> = Vec::new();
            let mut needs_descendant: Option<usize> = None;
            let mut deep_without_selector = false;
            let mut sequence_start = 0;
            let mut previous_combinator = None;

            while let Some((idx, part)) = iter.next() {
                // Find the start of the sequence
                if is_combinator!(part) {
                    previous_combinator = Some(idx);
                } else if let Some(_) = previous_combinator {
                    sequence_start = idx;
                    previous_combinator = None;
                }

                let Component::NonTSPseudoClass(PseudoClass::CustomFunction { name, .. }) = part else {
                    continue;
                };

                if *name != "deep" {
                    continue;
                }

                // The algorithm:
                // 1. Replace `:deep` with `[data-v-smth]`.
                // 2. Check if `:deep` was inside a bigger sequence or on its own:
                //    a) Bigger sequence (`.foo:deep`) - add Combinator::Descendant
                //       at the sequence start. The way lightningcss works
                //       is by using sequences like that (numbers are indices in the vec):
                //         3   4   5  2  0   1
                //       `.foo.bar.baz .qux:deep(#bar baz)`
                //
                //       The sequence start is at index `0`, so after adding a Descendant,
                //       we will end up with
                //         4   5   6  3  1   2            0
                //       `.foo.bar.baz .qux:deep(#bar baz) `
                //
                //       Now we parse the inner contents of `:deep` like that
                //         2  1 0
                //       `#bar baz`
                //
                //       And at step 3, we will append just like that
                //         7   8   9  6  4   5            3 2   1 0
                //       `.foo.bar.baz .qux:deep(#bar baz) #bar baz`
                //
                //       And, remember, we replaced `:deep` with `[data-v-smth]` at step 1
                //         7   8   9  6  4   5            3 2   1 0
                //       `.foo.bar.baz .qux[data-v-smth] #bar baz`
                //
                //    b) On its own (`.foo :deep`). Now this is a tricky part,
                //       because we need to move `:deep` to the end of the next sequence.
                //       For doing so, we will be swapping it with the next element till we find
                //       a combinator. Then, we will swap till the end of the sequence.
                //
                //       Take this example. I intentionally put `.qux` at the end
                //       to make sure we accept all kind of weird input (and because it
                //       allows `::v-deep` support):
                //         4   5   6  3  2            1  0
                //       `.foo.bar.baz :deep(#bar baz) .qux`
                //
                //       When we first swap `:deep` with Combinator::Descendant,
                //       interpretation of the Vec becomes questionable:
                //         3              4   5   6  2 1  0
                //       `:deep(#bar baz).foo.bar.baz   .qux`
                //
                //       We continue swapping till we find either end of vec or end of sequence:
                //         3   4   5    6            2 1  0
                //       `.foo.bar.baz:deep(#bar baz)   .qux`
                //
                //       Now, at step 3, we will be inserting contents of `:deep`
                //       at initial `sequence_start`,
                //       i.e. index of `:deep` in the beginning (in this case 2).
                //       The Combinator which now occupies index `sequence_start` will
                //       get moved because of `insert`, and remember about step 1:
                //
                //         6   7   8    9          5  4 3 2 1 0
                //       `.foo.bar.baz[data-v-smth] #bar baz .qux`
                //
                //    c) Special case (just `:deep(...)`).
                //       Technically, this is similar to case b),
                //       but since we cannot move it anywhere, we just will insert a Descendant
                //         0
                //       `:deep(#bar baz)`
                //
                // 3. Insert the parsed contents of `:deep` into the vec at `sequence_start`.
                //    Point 2 explains why it works.

                // Get the contents of `:deep` from cache
                let deep_contents = &cache[ptr];
                ptr += 1;

                // Step 1. Replace the contents
                *part = to_append!();

                // Check for special case - `:deep()` without selectors inside
                if deep_contents.is_empty() {
                    deep_without_selector = true;
                } else {
                    // Parse contents of `:deep` as a Selector
                    let Ok(parsed) = Selector::parse_string_with_options(
                        deep_contents,
                        Default::default()
                    ) else {
                        // return Err(VisitorError::ParsingDeepFailed);
                        continue;
                    };

                    // Just because `Selector.1` is a private field,
                    // we have to clone and use its public API to insert components
                    for part in parsed.iter_raw_match_order().cloned() {
                        components.push(part);
                    }
                }

                // Step 2. Determine the position of `:deep` inside the sequence
                // Is `:deep` its own sequence (`.foo :deep`) or not (`.foo.bar:deep`)?
                let is_deep_its_own_sequence = sequence_start == idx;

                // b) Part of other sequence is the simplest, just signify that Descendant is needed
                if !is_deep_its_own_sequence || selector_len == 1 {
                    // c) This is also true for cases where `:deep(#bar)` is the only component
                    if !is_deep_its_own_sequence || !components.is_empty() {
                        needs_descendant = Some(sequence_start)
                    }
                    break;
                }

                // a) `:deep` is its own sequence, we need to swap-move it
                // `.foo.bar :deep`     -> `.foo.bar[data-v-smth] `
                // `.foo.bar > :deep`   -> `.foo.bar[data-v-smth] >`
                // `.foo.bar + :deep`   -> `.foo.bar[data-v-smth] +`
                // `.foo.bar ~ :deep`   -> `.foo.bar[data-v-smth] ~`

                let mut encountered_combinator = false;
                let mut deep = &mut *part;

                while let Some((_, next_part)) = iter.next() {
                    if is_combinator!(next_part) {
                        if !encountered_combinator {
                            encountered_combinator = true
                        } else {
                            break;
                        }
                    }

                    std::mem::swap(deep, next_part);
                    deep = next_part;
                }

                break;
            }

            // Deep without selector already did its job
            if deep_without_selector {
                continue;
            }

            // Because current SFC compiler treats `.foo:deep` the same as `.foo :deep`
            if let Some(idx) = needs_descendant {
                selector.insert_raw(idx, Component::Combinator(Combinator::Descendant));
            }

            // When we have components, we have parsed `:deep`,
            // otherwise we have not and will just append to the right-most selector
            if components.len() > 0 {
                selector.insert_raw_multiple(sequence_start, components);
            } else {
                // this needs testing
                selector.append(to_append!())
            }
        }
    }
}

impl<'i> From<Error<ParserError<'i>>> for TransformError<'i> {
    fn from(value: Error<ParserError<'i>>) -> Self {
        TransformError::ParserError(value)
    }
}

impl From<Error<MinifyErrorKind>> for TransformError<'_> {
    fn from(value: Error<MinifyErrorKind>) -> Self {
        TransformError::MinifyError(value)
    }
}

impl From<Error<PrinterErrorKind>> for TransformError<'_> {
    fn from(value: Error<PrinterErrorKind>) -> Self {
        TransformError::PrinterError(value)
    }
}
