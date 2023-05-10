mod transform_style_scoped;

pub use transform_style_scoped::*;

#[allow(unused)]
mod tests {
    use lightningcss::{targets::Browsers, stylesheet::{MinifyOptions, PrinterOptions}};

    use crate::{transform_style_scoped::Transformer, TransformOptions};

    macro_rules! test_output {
        ($input: expr, $expected: expr, $options: expr) => {
            let mut transformer = Transformer::new($input, "abcd1234");
            let out = transformer.transform_style_scoped($options);

            let actual = match out {
                Ok(to_css_result) => Ok(to_css_result.code),
                Err(_) => Err(()),
            };

            assert_eq!(actual, $expected)
        };
    }

    macro_rules! test_ok {
        ($input: expr, $expected: expr, $options: expr) => {
            test_output!($input, Ok(String::from($expected)), $options);
        };
    }

    macro_rules! minify_yes {
        () => {
            TransformOptions {
                parse: Default::default(),
                minify: Some(MinifyOptions {
                    targets: Some(Browsers {
                        chrome: Some(100 << 16),
                        firefox: Some(100 << 16),
                        safari: Some(16 << 16),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                to_css: PrinterOptions {
                    minify: true,
                    ..Default::default()
                },
            }
        };
    }

    #[test]
    fn transform_style_scoped() {
        //
        // Without `:deep`
        //

        test_ok!(
            ".foo { background: yellow }",
            ".foo[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo > #bar baz { background: yellow }",
            ".foo>#bar baz[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo .bar, .foo > .baz, .foo + .foo, .foo ~ .qux { background: yellow }",
            ".foo .bar[data-v-abcd1234],.foo>.baz[data-v-abcd1234],.foo+.foo[data-v-abcd1234],.foo~.qux[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        //
        // With `:deep`
        //

        test_ok!(
            ":deep() { background: yellow }",
            "[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo:deep() { background: yellow }",
            ".foo[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo > #bar baz:deep() { background: yellow }",
            ".foo>#bar baz[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ":deep(#bar baz) { background: yellow }",
            "[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo :deep(#bar baz) { background: yellow }",
            ".foo[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo :deep(#bar baz), .qux { background: yellow }",
            ".foo[data-v-abcd1234] #bar baz,.qux[data-v-abcd1234]{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo :deep(#bar baz) .qux { background: yellow }",
            ".foo[data-v-abcd1234] #bar baz .qux{background:#ff0}",
            minify_yes!()
        );

        // Nobody should use `:deep` it like that
        test_ok!(
            ".foo :deep(#bar baz).bar .qux { background: yellow }",
            ".foo[data-v-abcd1234] #bar.bar baz .qux{background:#ff0}",
            minify_yes!()
        );

        // Vue sfc compiler treats `.foo:deep()` as `.foo :deep()`
        test_ok!(
            ".foo:deep(#bar baz) { background: yellow }",
            ".foo[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            ".foo .foo.bar .foo.bar.baz:deep(#bar baz) { background: yellow }",
            ".foo .foo.bar .foo.bar.baz[data-v-abcd1234] #bar baz{background:#ff0}",
            minify_yes!()
        );

        test_ok!(
            "#more.complex .selector > :deep(#bar baz) { background: yellow }",
            "#more.complex .selector[data-v-abcd1234]>#bar baz{background:#ff0}",
            minify_yes!()
        );
    }
}
