use fervid_core::OptionsApiBindings;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        Callee, Expr, ExprOrSpread, Module, ModuleDecl, ModuleItem, ObjectLit, PropOrSpread,
        SpreadElement,
    },
};

use crate::structs::VueResolvedImports;

mod analyzer;
mod components;
mod computed;
mod data;
mod directives;
mod emits;
mod exports;
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

pub struct ScriptOptionsTransformResult {
    pub vars: Box<OptionsApiBindings>,
    pub resolved_vue_imports: Box<VueResolvedImports>,
    pub default_export_obj: Option<ObjectLit>,
}

pub fn transform_and_record_script_options_api(
    module: &mut Module,
    opts: AnalyzeOptions,
) -> ScriptOptionsTransformResult {
    // Default export should be either an object or `defineComponent({ /* ... */ })`
    // let maybe_default_export = super::utils::find_default_export(module);
    let maybe_default_export = find_default_export_obj(module);

    // This is where we collect all the analyzed stuff
    let mut script_legacy_vars = OptionsApiBindings::default();
    let mut vue_imports = VueResolvedImports::default();

    // Analyze the imports and top level items
    if opts.collect_top_level_stmts {
        analyzer::analyze_top_level_items(module, &mut script_legacy_vars, &mut vue_imports)
    }

    // TODO The actual transformation?
    // Analyze the default export
    if let Some(ref default_export) = maybe_default_export {
        analyzer::analyze_default_export(default_export, &mut script_legacy_vars);
    }

    ScriptOptionsTransformResult {
        vars: Box::new(script_legacy_vars),
        resolved_vue_imports: Box::new(vue_imports),
        default_export_obj: maybe_default_export,
    }
}

/// Finds and takes ownership of the `export default` expression
fn find_default_export_obj(module: &mut Module) -> Option<ObjectLit> {
    let default_export_index = module
        .body
        .iter()
        .position(|module_item| match module_item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(_)) => true,
            _ => false,
        });

    let Some(idx) = default_export_index else {
        return None;
    };

    let item = module.body.remove(idx);
    // TODO What to do with weird default exports?
    let ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(expr)) = item else {
        unreachable!()
    };

    // TODO Unroll paren/seq, unroll `defineComponent` as in `fervid_script`
    let expr = unroll_default_export_expr(*expr.expr);

    match expr {
        // Object is the preferred syntax
        // export default { /* object fields */ }
        Expr::Object(obj_lit) => Some(obj_lit),

        // Call, Member are also supported
        // export default { ...expression }
        Expr::Member(_) | Expr::Call(_) => Some(ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Spread(SpreadElement {
                dot3_token: DUMMY_SP,
                expr: Box::new(expr),
            })],
        }),

        // Those are questionable
        // Expr::Cond(_) => todo!(),
        // Expr::Class(_) => todo!(),
        // Expr::Await(_) => todo!(),
        // Expr::TsTypeAssertion(_) => todo!(),
        // Expr::TsConstAssertion(_) => todo!(),
        // Expr::TsNonNull(_) => todo!(),
        // Expr::TsAs(_) => todo!(),
        // Expr::TsInstantiation(_) => todo!(),
        // Expr::TsSatisfies(_) => todo!(),

        // Everything else is invalid and should not be generated
        // TODO It would be better to also emit a hard error here
        _ => None,
    }
}

fn unroll_default_export_expr(mut expr: Expr) -> Expr {
    match expr {
        Expr::Call(ref mut call_expr) => {
            macro_rules! bail {
                () => {
                    return expr;
                };
            }

            // We only support `defineComponent` with 1 argument which isn't a spread
            if call_expr.args.len() != 1 {
                bail!();
            }

            let Callee::Expr(ref callee) = call_expr.callee else {
                bail!();
            };

            let Expr::Ident(callee_ident) = callee.as_ref() else {
                bail!();
            };

            // Todo compare against the imported symbol
            if &callee_ident.sym != "defineComponent" {
                bail!();
            }

            let is_first_arg_ok = matches!(call_expr.args[0], ExprOrSpread { spread: None, .. });
            if !is_first_arg_ok {
                bail!();
            }

            let Some(ExprOrSpread { spread: None, expr }) = call_expr.args.pop() else {
                unreachable!()
            };

            *expr
        }

        _ => expr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::parser::*;
    use fervid_core::{BindingTypes, FervidAtom, SetupBinding};

    fn analyze_js(input: &str, opts: AnalyzeOptions) -> ScriptOptionsTransformResult {
        let mut parsed = parse_javascript_module(input, 0, Default::default())
            .expect("analyze_js expects the input to be parseable")
            .0;

        let analyzed = transform_and_record_script_options_api(&mut parsed, opts);

        analyzed
    }

    fn analyze_ts(input: &str, opts: AnalyzeOptions) -> ScriptOptionsTransformResult {
        let mut parsed = parse_typescript_module(input, 0, Default::default())
            .expect("analyze_ts expects the input to be parseable")
            .0;

        let analyzed = transform_and_record_script_options_api(&mut parsed, opts);

        analyzed
    }

    macro_rules! test_js_and_ts {
        ($input: expr, $expected: expr) => {
            assert_eq!(*analyze_js($input, Default::default()).vars, $expected);
            assert_eq!(*analyze_ts($input, Default::default()).vars, $expected);
        };

        ($input: expr, $expected: expr, $opts: expr) => {
            assert_eq!(*analyze_js($input, $opts.clone()).vars, $expected);
            assert_eq!(*analyze_ts($input, $opts.clone()).vars, $expected);
        };
    }

    #[test]
    fn it_detects_export_default() {
        // Empty bindings are expected when empty `export default` is found
        let no_bindings = OptionsApiBindings::default();
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
                let mut parsed = parse_javascript_module($input, 0, Default::default())
                    .expect("parsing js should not err")
                    .0;
                assert_eq!(
                    transform_and_record_script_options_api(
                        &mut parsed,
                        AnalyzeOptions {
                            require_default_export: true,
                            ..Default::default()
                        }
                    )
                    .default_export_obj,
                    None
                );

                let mut parsed = parse_typescript_module($input, 0, Default::default())
                    .expect("parsing ts should not err")
                    .0;
                assert_eq!(
                    transform_and_record_script_options_api(
                        &mut parsed,
                        AnalyzeOptions {
                            require_default_export: true,
                            ..Default::default()
                        }
                    )
                    .default_export_obj,
                    None
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

        // TODO I am not too sure how these expressions should be treated.
        // Currently they are not being analyzed, and only being added as `{ ...callExpression() }`
        // should_err!("export default defineComponent()");
        // should_err!("export default defineComponent(42)");
        // should_err!("export default wrongDefineComponent({})");
    }

    #[test]
    fn it_sees_name() {
        let test_name = OptionsApiBindings {
            name: Some(FervidAtom::from("TestComponent")),
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
            OptionsApiBindings {
                components: vec![
                    FervidAtom::from("Foo"),
                    FervidAtom::from("FooBar"),
                    FervidAtom::from("Baz")
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
                    lorem: 'not a valid computed but should be analyzed',
                    getterSetter: {
                        get() {},
                        set() {},
                    }
                }
            }
            ",
            OptionsApiBindings {
                computed: vec![
                    FervidAtom::from("foo"),
                    FervidAtom::from("bar"),
                    FervidAtom::from("lorem"),
                    FervidAtom::from("getterSetter")
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_data() {
        let expected = OptionsApiBindings {
            data: vec![
                FervidAtom::from("foo"),
                FervidAtom::from("bar"),
                FervidAtom::from("baz"),
                FervidAtom::from("qux"),
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
            OptionsApiBindings {
                directives: vec![FervidAtom::from("foo"), FervidAtom::from("bar")],
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
            OptionsApiBindings {
                emits: vec![
                    FervidAtom::from("foo"),
                    FervidAtom::from("bar"),
                    FervidAtom::from("baz")
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
            OptionsApiBindings {
                emits: vec![FervidAtom::from("foo"), FervidAtom::from("bar")],
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
            OptionsApiBindings {
                expose: vec![
                    FervidAtom::from("foo"),
                    FervidAtom::from("bar"),
                    FervidAtom::from("baz")
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
            OptionsApiBindings {
                inject: vec![
                    FervidAtom::from("foo"),
                    FervidAtom::from("bar"),
                    FervidAtom::from("baz")
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
            OptionsApiBindings {
                inject: vec![FervidAtom::from("foo"), FervidAtom::from("bar")],
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
            OptionsApiBindings {
                methods: vec![FervidAtom::from("foo"), FervidAtom::from("bar")],
                ..Default::default()
            }
        );
    }

    #[test]
    fn it_analyzes_props() {
        let expected = OptionsApiBindings {
            props: vec![
                FervidAtom::from("foo"),
                FervidAtom::from("bar"),
                FervidAtom::from("baz"),
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
                props: ['foo', "bar", `baz`, variable, `non${'trivial'}`, Symbol()]
            }"#,
            expected
        );

        // No props
        let no_bindings = OptionsApiBindings::default();

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

        test_js_and_ts!(
            r"
            export default {
                props: () => {}
            }",
            no_bindings
        );
    }

    #[test]
    fn it_analyzes_setup() {
        let expected = OptionsApiBindings {
            setup: vec![
                SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("bar"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupMaybeRef),
                SetupBinding(FervidAtom::from("pi"), BindingTypes::SetupMaybeRef),
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
            OptionsApiBindings {
                props: vec![FervidAtom::from("foo"), FervidAtom::from("bar")],
                data: vec![FervidAtom::from("hello")],
                setup: vec![
                    SetupBinding(FervidAtom::from("inputModel"), BindingTypes::SetupMaybeRef),
                    SetupBinding(FervidAtom::from("modelValue"), BindingTypes::SetupMaybeRef),
                    SetupBinding(FervidAtom::from("list"), BindingTypes::SetupMaybeRef),
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
            OptionsApiBindings {
                setup: vec![
                    SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupRef),
                    SetupBinding(FervidAtom::from("bar"), BindingTypes::SetupRef),
                    SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupReactiveConst),
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
            OptionsApiBindings {
                setup: vec![
                    SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupRef),
                    SetupBinding(FervidAtom::from("bar"), BindingTypes::SetupRef),
                    SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupReactiveConst),
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
            OptionsApiBindings {
                setup: vec![
                    SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupMaybeRef),
                    SetupBinding(FervidAtom::from("bar"), BindingTypes::SetupMaybeRef),
                    SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupMaybeRef),
                ],
                imports: vec![
                    SetupBinding(FervidAtom::from("ref"), BindingTypes::Imported),
                    SetupBinding(FervidAtom::from("computed"), BindingTypes::Imported),
                    SetupBinding(FervidAtom::from("reactive"), BindingTypes::Imported),
                ],
                ..Default::default()
            },
            opts
        );
    }

    #[test]
    fn it_analyzes_top_level_exports() {
        let opts = AnalyzeOptions {
            require_default_export: false,
            collect_top_level_stmts: true,
        };

        // Different types of exports
        test_js_and_ts!(
            r"
            export * as foo from '@loremipsum/foo'
            // export bar from 'mod-bar' // is this a valid syntax?
            export { default as baz, qux } from './rest'
            ",
            OptionsApiBindings {
                setup: vec![
                    SetupBinding(FervidAtom::from("foo"), BindingTypes::SetupMaybeRef),
                    SetupBinding(FervidAtom::from("baz"), BindingTypes::SetupMaybeRef),
                    SetupBinding(FervidAtom::from("qux"), BindingTypes::SetupMaybeRef),
                ],

                ..Default::default()
            },
            opts
        );

        // Type-only exports should be ignored
        assert_eq!(
            *analyze_ts(
                r"
                export type * as foo from '@loremipsum/foo'
                export type { bar } from 'mod-bar'
                export { type default as baz, type qux } from './rest'
                ",
                opts
            )
            .vars,
            OptionsApiBindings::default()
        );
    }
}
