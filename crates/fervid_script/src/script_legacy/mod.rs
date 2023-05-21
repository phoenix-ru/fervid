use swc_core::ecma::{
    ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, Expr, Function, Lit, Module, ObjectLit, Prop,
        PropName, PropOrSpread,
    },
    atoms::JsWord,
};

use crate::atoms::*;
use self::{
    components::collect_components_object,
    computed::collect_computed_object,
    data::{collect_data_bindings_block_stmt, collect_data_bindings_expr},
    directives::collect_directives_object,
    emits::{collect_emits_bindings_array, collect_emits_bindings_object},
    expose::collect_expose_bindings_array,
    inject::{collect_inject_bindings_array, collect_inject_bindings_object},
    methods::collect_methods_object,
    props::{collect_prop_bindings_array, collect_prop_bindings_object},
    setup::{collect_setup_bindings_block_stmt, collect_setup_bindings_expr},
    utils::find_default_export,
};

pub use structs::ScriptLegacyVars;

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
mod structs;
pub mod utils;

pub fn analyze_script_legacy(module: &Module) -> Result<ScriptLegacyVars, ()> {
    // This is where we collect all the analyzed stuff
    let mut script_legacy_vars = ScriptLegacyVars::default();

    // Default export should be either an object or `defineComponent({ /* ... */ })`
    let Some(default_export) = find_default_export(module) else {
        return Err(());
    };

    // tl;dr Visit every method, arrow function, object or array and forward control
    for field in default_export.props.iter() {
        let PropOrSpread::Prop(prop) = field else {
            continue;
        };

        match **prop {
            Prop::KeyValue(ref key_value) => {
                let sym = match key_value.key {
                    PropName::Ident(ref ident) => &ident.sym,
                    PropName::Str(ref s) => &s.value,
                    _ => continue,
                };

                match *key_value.value {
                    Expr::Array(ref array_lit) => {
                        handle_options_array(sym, array_lit, &mut script_legacy_vars)
                    }
                    Expr::Object(ref obj_lit) => {
                        handle_options_obj(sym, obj_lit, &mut script_legacy_vars)
                    }
                    Expr::Fn(ref fn_expr) => {
                        handle_options_function(sym, &fn_expr.function, &mut script_legacy_vars)
                    }
                    Expr::Arrow(ref arrow_expr) => {
                        handle_options_arrow_function(sym, arrow_expr, &mut script_legacy_vars)
                    }
                    Expr::Lit(ref lit) => handle_options_lit(sym, lit, &mut script_legacy_vars),

                    // These latter types technically can be analyzed as well,
                    // because they only need `.expr` unwrapping and re-matching.
                    // It can be done when the match moves into a function
                    // which can be recursively called.
                    // Expr::TsTypeAssertion(_) => todo!(),
                    // Expr::TsConstAssertion(_) => todo!(),
                    // Expr::TsAs(_) => todo!(),
                    _ => {
                        continue;
                    }
                }
            }
            Prop::Method(ref method) => {
                let sym = match method.key {
                    PropName::Ident(ref ident) => &ident.sym,
                    PropName::Str(ref s) => &s.value,
                    _ => continue,
                };

                handle_options_function(sym, &method.function, &mut script_legacy_vars)
            }
            _ => {}
        }
    }

    Ok(script_legacy_vars)
}

pub fn transform_script_legacy(_module: &mut Module) {
    todo!()
}

/// In Options API, `props`, `inject`, `emits` and `expose` may be arrays
fn handle_options_array(
    field: &JsWord,
    array_lit: &ArrayLit,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    if *field == *PROPS {
        collect_prop_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *INJECT {
        collect_inject_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *EMITS {
        collect_emits_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *EXPOSE {
        collect_expose_bindings_array(array_lit, script_legacy_vars)
    }
}

/// Similar to [handle_options_array], only `data`, `setup` may be declared as arrow fns
fn handle_options_arrow_function(
    field: &JsWord,
    arrow_expr: &ArrowExpr,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    // Arrow functions may either have a body or an expression
    // `() => {}` is a body which returns nothing
    // `() => ({})` is an expression which returns an empty object
    macro_rules! forward_block_stmt_or_expr {
        ($forward_block_stmt: ident, $forward_expr: ident) => {
            match *arrow_expr.body {
                BlockStmtOrExpr::BlockStmt(ref block_stmt) => {
                    $forward_block_stmt(block_stmt, script_legacy_vars)
                }
                BlockStmtOrExpr::Expr(ref arrow_body_expr) => {
                    $forward_expr(arrow_body_expr, script_legacy_vars)
                }
            }
        };
    }

    // It reads a bit cryptic because of the macro calls,
    // but you should only care about the functions which are called,
    // e.g. [`collect_data_bindings_block_stmt`]
    if *field == *DATA {
        forward_block_stmt_or_expr!(collect_data_bindings_block_stmt, collect_data_bindings_expr);
    } else if *field == *SETUP {
        forward_block_stmt_or_expr!(
            collect_setup_bindings_block_stmt,
            collect_setup_bindings_expr
        )
    }
}

/// Same as [handle_options_arrow_function], `data` and `setup`
fn handle_options_function(
    field: &JsWord,
    function: &Function,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    let Some(ref function_body) = function.body else {
        return;
    };

    if *field == *DATA {
        collect_data_bindings_block_stmt(function_body, script_legacy_vars)
    } else if *field == *SETUP {
        collect_setup_bindings_block_stmt(function_body, script_legacy_vars)
    }
}

/// `name`
fn handle_options_lit(field: &JsWord, lit: &Lit, script_legacy_vars: &mut ScriptLegacyVars) {
    if *field == *NAME {
        if let Lit::Str(name) = lit {
            script_legacy_vars.name = Some(name.value.to_owned())
        }
    }
}

/// `props`, `computed`, `inject`, `emits`, `components`, `methods`, `directives`
fn handle_options_obj(
    field: &JsWord,
    obj_lit: &ObjectLit,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    if *field == *PROPS {
        collect_prop_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *COMPUTED {
        collect_computed_object(obj_lit, script_legacy_vars)
    } else if *field == *INJECT {
        collect_inject_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *EMITS {
        collect_emits_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *COMPONENTS {
        collect_components_object(obj_lit, script_legacy_vars)
    } else if *field == *METHODS {
        collect_methods_object(obj_lit, script_legacy_vars)
    } else if *field == *DIRECTIVES {
        collect_directives_object(obj_lit, script_legacy_vars)
    }
}

#[cfg(test)]
mod tests {
    use swc_core::ecma::atoms::JsWord;
    use crate::parser::*;
    use super::*;

    fn analyze_js(input: &str) -> ScriptLegacyVars {
        let parsed = parse_javascript_module(input, 0, Default::default())
            .expect("analyze_js expects the input to be parseable")
            .0;

        let analyzed = analyze_script_legacy(&parsed)
            .expect("analyze_js expects the input to be analyzed successfully");

        analyzed
    }

    fn analyze_ts(input: &str) -> ScriptLegacyVars {
        let parsed = parse_typescript_module(input, 0, Default::default())
            .expect("analyze_ts expects the input to be parseable")
            .0;

        let analyzed = analyze_script_legacy(&parsed)
            .expect("analyze_ts expects the input to be analyzed successfully");

        analyzed
    }

    macro_rules! test_js_and_ts {
        ($input: expr, $expected: expr) => {
            assert_eq!(analyze_js($input), $expected);
            assert_eq!(analyze_ts($input), $expected);
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
                assert_eq!(analyze_script_legacy(&parsed), Err(()));

                let parsed = parse_typescript_module($input, 0, Default::default())
                    .expect("parsing ts should not err")
                    .0;
                assert_eq!(analyze_script_legacy(&parsed), Err(()))
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
                JsWord::from("foo"),
                JsWord::from("bar"),
                JsWord::from("baz"),
                JsWord::from("pi"),
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
                    JsWord::from("inputModel"),
                    JsWord::from("modelValue"),
                    JsWord::from("list")
                ],
                ..Default::default()
            }
        );
    }
}
