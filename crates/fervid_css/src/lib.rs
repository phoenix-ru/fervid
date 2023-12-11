//! Style transformer for Vue `<style>` blocks
//!
//! ## Example
//! ```
//! use swc_core::common::{Span, BytePos};
//!
//! let input = r#"
//! .example {
//!   background: #ff0;
//! }
//! "#;
//!
//! // Note: `Span` usually comes from the input, e.g. from `<style>` block
//! let span = Span::new(
//!     BytePos(1),
//!     BytePos(1 + input.len() as u32),
//!     Default::default(),
//! );
//! let mut errors = Vec::new();
//!
//! let result = fervid_css::transform_css(input, span, Some("data-v-abcd1234"), &mut errors, Default::default());
//!
//! if let Some(transformed_css) = result {
//!     assert_eq!(".example[data-v-abcd1234]{background:#ff0}", transformed_css);
//! }
//! ```

mod css;

pub use css::*;

#[cfg(test)]
#[allow(unused)]
mod tests {
    use swc_core::common::{Span, BytePos};
    use crate::css;

    macro_rules! test_output {
        ($input: expr, $expected: expr, $options: expr) => {
            // lightningcss
            // let mut transformer = Transformer::new($input, "data-v-abcd1234");
            // let out = transformer.transform_style_scoped($options);
            // let actual = match out {
            //     Ok(to_css_result) => Ok(to_css_result.code),
            //     Err(_) => Err(()),
            // };
            // assert_eq!(actual, $expected);

            // SWC
            let span = Span::new(
                BytePos(1),
                BytePos(1 + $input.len() as u32),
                Default::default(),
            );
            let mut errors = Vec::new();
            let out = css::transform_css($input, span, Some("data-v-abcd1234"), &mut errors, Default::default());
            assert_eq!(out.ok_or(()), $expected);
        };
    }

    macro_rules! test_ok {
        ($input: expr, $expected: expr, $options: expr) => {
            test_output!($input, Ok(String::from($expected)), $options);
        };
    }

    macro_rules! minify_yes {
        () => {
            // TransformOptions {
            //     parse: Default::default(),
            //     minify: Some(MinifyOptions {
            //         targets: Some(Browsers {
            //             chrome: Some(100 << 16),
            //             firefox: Some(100 << 16),
            //             safari: Some(16 << 16),
            //             ..Default::default()
            //         })
            //         .into(),
            //         ..Default::default()
            //     }),
            //     to_css: PrinterOptions {
            //         minify: true,
            //         ..Default::default()
            //     },
            // }
        };
    }

    #[test]
    fn transform_style_scoped() {
        //
        // Without `:deep`
        //

        test_ok!(
            ".foo { background: #ff0 }",
            ".foo[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo > #bar baz { background: #ff0 }",
            ".foo>#bar baz[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo .bar, .foo > .baz, .foo + .foo, .foo ~ .qux { background: #ff0 }",
            ".foo .bar[data-v-abcd1234],.foo>.baz[data-v-abcd1234],.foo+.foo[data-v-abcd1234],.foo~.qux[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        //
        // With `:deep`
        //

        test_ok!(
            ":deep() { background: #ff0 }",
            "[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo:deep() { background: #ff0 }",
            ".foo[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo > #bar baz:deep() { background: #ff0 }",
            ".foo>#bar baz[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ":deep(#bar baz) { background: #ff0 }",
            "[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo :deep(#bar baz) { background: #ff0 }",
            ".foo[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo :deep(#bar baz), .qux { background: #ff0 }",
            ".foo[data-v-abcd1234] #bar baz,.qux[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo :deep(#bar baz) .qux { background: #ff0 }",
            ".foo[data-v-abcd1234] #bar baz .qux{background:#ff0}",
            minify_yes!()
        );

        // Nobody should use `:deep` like that
        // test_ok!(
        //     ".foo :deep(#bar baz).bar .qux { background: #ff0 }",
        //     ".foo[data-v-abcd1234] #bar.bar baz .qux{background:#ff0}",
        //     minify_yes!()
        // );

        // Vue sfc compiler treats `.foo:deep()` as `.foo :deep()`
        test_ok!(
            ".foo:deep(#bar baz) { background: #ff0 }",
            ".foo[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );
        test_ok!(
            ".foo:deep(#bar baz) .qux { background: #ff0 }",
            ".foo[data-v-abcd1234] #bar baz .qux{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo .foo.bar .foo.bar.baz:deep(#bar baz) { background: #ff0 }",
            ".foo .foo.bar .foo.bar.baz[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            "#more.complex .selector > :deep(#bar baz) { background: #ff0 }",
            "#more.complex .selector[data-v-abcd1234]>#bar baz{background:#ff0}",
            minify_yes!()
        );

        //
        // At-rules
        //
        test_ok!(
            "@media screen and (min-width: 500px) { .foo { background: #ff0 } }",
            "@media screen and (min-width:500px){.foo[data-v-abcd1234]{background:#ff0}}",
            minify_yes!()
        );
    }
}
