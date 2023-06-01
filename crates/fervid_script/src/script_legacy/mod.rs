use swc_core::ecma::ast::Module;

use crate::{structs::{ScriptLegacyVars, VueResolvedImports}, common::utils::find_default_export};

mod analyzer;
mod components;
mod computed;
mod data;
mod directives;
mod emits;
mod expose;
mod inject;
mod methods;
mod props;
mod setup;

#[derive(Default, Clone)]
pub struct AnalyzeOptions {
    /// Setting this to `true` will cause `analyze_script_legacy`
    /// to return Err if no default export was found
    pub require_default_export: bool,
    /// When `true`, all the top-level statements will be
    /// analyzed as if they are directly available to template via setup (same as in `<script setup>`)
    ///
    /// TODO: Is it really correct to put these statements into `setup`?
    /// In `PROD` mode they are available to the inline template as module globals,
    /// in `DEV` mode they are available under `$setup` because of `__returned` object
    pub collect_top_level_stmts: bool,
}

pub fn analyze_script_legacy(
    module: &Module,
    opts: AnalyzeOptions,
) -> Result<ScriptLegacyVars, ()> {
    // Default export should be either an object or `defineComponent({ /* ... */ })`
    let maybe_default_export = find_default_export(module);

    // Sometimes we care about default export, e.g. in tests
    if let (None, true) = (maybe_default_export, opts.require_default_export) {
        return Err(());
    }

    // This is where we collect all the analyzed stuff
    let mut script_legacy_vars = ScriptLegacyVars::default();
    let mut vue_imports = VueResolvedImports::default();

    // Analyze the imports and top level items
    if opts.collect_top_level_stmts {
        analyzer::analyze_top_level_items(module, &mut script_legacy_vars, &mut vue_imports)
    }

    // Analyze the default export
    if let Some(default_export) = maybe_default_export {
        analyzer::analyze_default_export(default_export, &mut script_legacy_vars)
    }

    Ok(script_legacy_vars)
}

pub fn transform_script_legacy(_module: &mut Module) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parser::*,
        structs::{BindingTypes, SetupBinding},
    };
    use swc_core::{ecma::atoms::JsWord, common::SyntaxContext};

    fn analyze_js(input: &str, opts: AnalyzeOptions) -> ScriptLegacyVars {
        let parsed = parse_javascript_module(input, 0, Default::default())
            .expect("analyze_js expects the input to be parseable")
            .0;

        let analyzed = analyze_script_legacy(&parsed, opts)
            .expect("analyze_js expects the input to be analyzed successfully");

        analyzed
    }

    fn analyze_ts(input: &str, opts: AnalyzeOptions) -> ScriptLegacyVars {
        let parsed = parse_typescript_module(input, 0, Default::default())
            .expect("analyze_ts expects the input to be parseable")
            .0;

        let analyzed = analyze_script_legacy(&parsed, opts)
            .expect("analyze_ts expects the input to be analyzed successfully");

        analyzed
    }

    macro_rules! test_js_and_ts {
        ($input: expr, $expected: expr) => {
            assert_eq!(analyze_js($input, Default::default()), $expected);
            assert_eq!(analyze_ts($input, Default::default()), $expected);
        };

        ($input: expr, $expected: expr, $opts: expr) => {
            assert_eq!(analyze_js($input, $opts.clone()), $expected);
            assert_eq!(analyze_ts($input, $opts.clone()), $expected);
        };
    }

    #[test]
    fn it_detects_export_default() {
        // Empty bindings are expected when empty `export default` is found
        let no_bindings = ScriptLegacyVars::default();
        test_js_and_ts!("export default {}", no_bindings);
        test_js_and_ts!("export default defineComponent({})", no_bindings);
        test_js_and_ts!(
            r"
            import { ref } from 'vue'
            export default {}
            ",
            no_bindings
        );
        test_js_and_ts!(
            r"
            import { defineComponent, ref } from 'vue'
            export default defineComponent({})
            ",
            no_bindings
        );
    }

    /// Analysis should return `Err` when suitable default export was not found.
    /// But parsing should not fail.
    #[test]
    fn it_errs_when_export_default_is_invalid() {
        macro_rules! should_err {
            ($input: expr) => {
                let parsed = parse_javascript_module($input, 0, Default::default())
                    .expect("parsing js should not err")
                    .0;
                assert_eq!(
                    analyze_script_legacy(
                        &parsed,
                        AnalyzeOptions {
                            require_default_export: true,
                            ..Default::default()
                        }
                    ),
                    Err(())
                );

                let parsed = parse_typescript_module($input, 0, Default::default())
                    .expect("parsing ts should not err")
                    .0;
                assert_eq!(
                    analyze_script_legacy(
                        &parsed,
                        AnalyzeOptions {
                            require_default_export: true,
                            ..Default::default()
                        }
                    ),
                    Err(())
                )
            };
        }

        should_err!("");
        should_err!("export default 42");
        should_err!("export default 'foo'");
        should_err!("export default function() { /* ... */ }");
        should_err!(
            r"
            import { ref } from 'vue'
            export default function() { /* ... */ }
            "
        );
        should_err!("export default defineComponent()");
        should_err!("export default defineComponent(42)");
        should_err!("export default wrongDefineComponent({})");
    }

    #[test]
    fn it_sees_name() {
        let test_name = ScriptLegacyVars {
            name: Some(JsWord::from("TestComponent")),
            ..Default::default()
        };

        test_js_and_ts!(r"export default { name: 'TestComponent' }", test_name);
        test_js_and_ts!(r#"export default { name: "TestComponent" }"#, test_name);
        test_js_and_ts!(r"export default { name: `TestComponent` }", test_name);

        test_js_and_ts!(
            r"export default defineComponent({ name: 'TestComponent' })",
            test_name
        );
    }

    #[test]
    fn it_analyzes_components() {
        test_js_and_ts!(
            r"
            export default {
                components: {
                    Foo,
                    FooBar,
                    Baz: Qux
                }
            }
            ",
            ScriptLegacyVars {
                components: vec![
                    JsWord::from("Foo"),
                    JsWord::from("FooBar"),
                    JsWord::from("Baz")
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_computed() {
        test_js_and_ts!(
            r"
            export default {
                computed: {
                    foo() {
                        return this.bar
                    },
                    bar: () => 'baz' + 'qux',
                    lorem: 'not a valid computed but should be analyzed'
                }
            }
            ",
            ScriptLegacyVars {
                computed: vec![
                    JsWord::from("foo"),
                    JsWord::from("bar"),
                    JsWord::from("lorem")
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_data() {
        let expected = ScriptLegacyVars {
            data: vec![
                JsWord::from("foo"),
                JsWord::from("bar"),
                JsWord::from("baz"),
                JsWord::from("qux"),
            ],
            ..Default::default()
        };

        test_js_and_ts!(
            r"
            export default {
                data() {
                    const foo = 'foo'

                    return {
                        foo,
                        bar: 42,
                        'baz': false,
                        qux() {}
                    }
                }
            }
            ",
            expected
        );

        test_js_and_ts!(
            r"
            const foo = 'foo'
            export default {
                data: () => ({
                    foo,
                    bar: 42,
                    'baz': false,
                    qux() {}
                })
            }
            ",
            expected
        );
    }

    #[test]
    fn it_analyzes_directives() {
        test_js_and_ts!(
            r"
            export default {
                directives: { foo, bar: {} }
            }
            ",
            ScriptLegacyVars {
                directives: vec![JsWord::from("foo"), JsWord::from("bar")],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_emits() {
        test_js_and_ts!(
            r#"
            export default {
                emits: ['foo', "bar", `baz`, `non${'trivial'}`]
            }
            "#,
            ScriptLegacyVars {
                emits: vec![
                    JsWord::from("foo"),
                    JsWord::from("bar"),
                    JsWord::from("baz")
                ],
                ..Default::default()
            }
        );

        test_js_and_ts!(
            r#"
            export default {
                emits: { foo: null, bar: (v) => !!v }
            }
            "#,
            ScriptLegacyVars {
                emits: vec![JsWord::from("foo"), JsWord::from("bar")],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_expose() {
        test_js_and_ts!(
            r#"
            export default {
                expose: ['foo', "bar", `baz`, `non${'trivial'}`]
            }
            "#,
            ScriptLegacyVars {
                expose: vec![
                    JsWord::from("foo"),
                    JsWord::from("bar"),
                    JsWord::from("baz")
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_inject() {
        test_js_and_ts!(
            r#"
            export default {
                inject: ['foo', "bar", `baz`, `non${'trivial'}`]
            }
            "#,
            ScriptLegacyVars {
                inject: vec![
                    JsWord::from("foo"),
                    JsWord::from("bar"),
                    JsWord::from("baz")
                ],
                ..Default::default()
            }
        );

        test_js_and_ts!(
            r#"
            export default {
                inject: { foo: 'foo', bar: { from: 'baz' } }
            }
            "#,
            ScriptLegacyVars {
                inject: vec![JsWord::from("foo"), JsWord::from("bar")],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_methods() {
        test_js_and_ts!(
            r"
            export default {
                methods: {
                    foo() {
                        console.log('foo called')
                    },
                    bar: () => prompt('Bar called?')
                }
            }
            ",
            ScriptLegacyVars {
                methods: vec![JsWord::from("foo"), JsWord::from("bar")],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_props() {
        let expected = ScriptLegacyVars {
            props: vec![
                JsWord::from("foo"),
                JsWord::from("bar"),
                JsWord::from("baz"),
            ],
            ..Default::default()
        };

        // Array syntax
        test_js_and_ts!(
            r#"
            export default {
                props: ['foo', "bar", `baz`]
            }"#,
            expected
        );

        // Obj + Types
        test_js_and_ts!(
            r#"
            export default {
                props: {
                    foo: String,
                    'bar': Number,
                    "baz": null
                }
            }"#,
            expected
        );

        // Obj + Empty objs
        test_js_and_ts!(
            r#"
            export default {
                props: {
                    foo: {},
                    'bar': {},
                    "baz": {}
                }
            }"#,
            expected
        );

        // Obj + Fully-qualified
        test_js_and_ts!(
            r#"
            export default {
                props: {
                    foo: { type: String },
                    'bar': { type: Number },
                    "baz": { type: [String, Number] }
                }
            }"#,
            expected
        );

        // Non-trivial keys are ignored
        test_js_and_ts!(
            r#"
            export default {
                props: {
                    foo: String,
                    'bar': Number,
                    "baz": null,
                    [nontrivial]: String,
                    [Symbol()]: String
                }
            }"#,
            expected
        );
        test_js_and_ts!(
            r#"
            export default {
                props: ['foo', "bar", `baz`, `non${'trivial'}`, Symbol()]
            }"#,
            expected
        );

        // No props
        let no_bindings = ScriptLegacyVars::default();

        test_js_and_ts!(
            r"
            export default {
                props: []
            }",
            no_bindings
        );

        test_js_and_ts!(
            r"
            export default {
                props: {}
            }",
            no_bindings
        );
    }

    #[test]
    fn it_analyzes_setup() {
        let expected = ScriptLegacyVars {
            setup: vec![
                SetupBinding(JsWord::from("foo"), BindingTypes::SetupMaybeRef),
                SetupBinding(JsWord::from("bar"), BindingTypes::SetupMaybeRef),
                SetupBinding(JsWord::from("baz"), BindingTypes::SetupMaybeRef),
                SetupBinding(JsWord::from("pi"), BindingTypes::SetupMaybeRef),
            ],
            ..Default::default()
        };

        test_js_and_ts!(
            r#"
            import { ref, computed, reactive } from 'vue'

            export default {
                setup() {
                    console.log('white noise')

                    const foo = ref('foo')
                    const bar = computed(() => 42)

                    return {
                        foo,
                        'bar': bar,
                        "baz": reactive({
                            shouldNotBeIncluded: true
                        }),
                        pi: 3.14
                    }
                }
            }
            "#,
            expected
        );

        test_js_and_ts!(
            r#"
            import { ref, computed, reactive } from 'vue'

            export default {
                setup: () => {
                    console.log('white noise')

                    const foo = ref('foo')
                    const bar = computed(() => 42)

                    return {
                        foo,
                        'bar': bar,
                        "baz": reactive({
                            shouldNotBeIncluded: true
                        }),
                        pi: 3.14
                    }
                }
            }
            "#,
            expected
        );

        test_js_and_ts!(
            r"
            import { ref, computed, reactive } from 'vue'

            export default {
                setup: () => ({
                    foo: ref('foo'),
                    bar: computed(() => 42),
                    baz: reactive({
                        shouldNotBeIncluded: true
                    }),
                    pi: 3.14
                })
            }
            ",
            expected
        );

        test_js_and_ts!(
            r#"
            import { ref, computed, reactive } from 'vue'

            export default {
                async setup() {
                    await ((async function() {
                        return {
                            confusion: true
                        }
                    })())

                    console.log('white noise')

                    const foo = ref('foo')
                    const bar = computed(() => 42)

                    return {
                        foo,
                        'bar': bar,
                        "baz": reactive({
                            shouldNotBeIncluded: true
                        }),
                        pi: 3.14
                    }
                }
            }
            "#,
            expected
        );
    }

    #[test]
    fn it_analyzes_everything() {
        let input = r"
        import { defineComponent, ref } from 'vue'

        export default defineComponent({
            props: ['foo', 'bar'],
            data() {
                return {
                    hello: 'world'
                }
            },
            setup() {
                const inputModel = ref('')
                const modelValue = ref('')
                const list = [1, 2, 3]

                return {
                    inputModel,
                    modelValue,
                    list
                }
            },
        })
        ";

        test_js_and_ts!(
            input,
            ScriptLegacyVars {
                props: vec![JsWord::from("foo"), JsWord::from("bar")],
                data: vec![JsWord::from("hello")],
                setup: vec![
                    SetupBinding(JsWord::from("inputModel"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("modelValue"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("list"), BindingTypes::SetupMaybeRef),
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_top_level() {
        let opts = AnalyzeOptions {
            require_default_export: false,
            collect_top_level_stmts: true,
        };

        // Regular usage
        test_js_and_ts!(
            r"
            import { ref, computed, reactive } from 'vue'

            const foo = ref(42)
            const bar = computed(() => 'vue computed')
            const baz = reactive({ qux: true })
            ",
            ScriptLegacyVars {
                setup: vec![
                    SetupBinding(JsWord::from("foo"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("bar"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("baz"), BindingTypes::SetupReactiveConst),
                ],
                ..Default::default()
            },
            opts
        );

        // Aliased usage
        test_js_and_ts!(
            r"
            import { ref as rf, computed as cm, reactive as ra } from 'vue'

            const foo = rf(42)
            const bar = cm(() => 'vue computed')
            const baz = ra({ qux: true })
            ",
            ScriptLegacyVars {
                setup: vec![
                    SetupBinding(JsWord::from("foo"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("bar"), BindingTypes::SetupRef),
                    SetupBinding(JsWord::from("baz"), BindingTypes::SetupReactiveConst),
                ],
                ..Default::default()
            },
            opts
        );

        // Usage not from main package should not be recognized as vue
        test_js_and_ts!(
            r"
            import { ref } from 'vue-but-not-really'
            import { computed } from './vue'
            import { reactive } from 'vue/some/internals'

            const foo = ref(42)
            const bar = computed(() => 'vue computed')
            const baz = reactive({ qux: true })
            ",
            ScriptLegacyVars {
                setup: vec![
                    SetupBinding(JsWord::from("foo"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("bar"), BindingTypes::SetupMaybeRef),
                    SetupBinding(JsWord::from("baz"), BindingTypes::SetupMaybeRef),
                ],
                imports: vec![
                    (JsWord::from("ref"), SyntaxContext::default()),
                    (JsWord::from("computed"), SyntaxContext::default()),
                    (JsWord::from("reactive"), SyntaxContext::default()),
                ],
                ..Default::default()
            },
            opts
        );
    }
}
