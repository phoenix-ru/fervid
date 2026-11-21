use fervid::{CompileOptions, CompileResult, compile};
use fervid_core::BindingTypes;

// Fervid improvement over Vue's SFC nested-rest metadata bug
// https://github.com/vuejs/core/blob/8b479a89445868d474f17e2d837765e7c9ece5e4/packages/compiler-sfc/src/compileScript.ts#L1240-L1311
#[test]
fn nested_rest_setup_bindings_reach_render_and_inline_templates() {
    let source = r#"
<script setup>
const [x, ...[y]] = getValues()
</script>
<template>{{ y }}</template>
"#;

    for is_prod in [false, true] {
        let result = compile_sfc(source, is_prod);
        let bindings: Vec<_> = result
            .setup_bindings
            .iter()
            .map(|binding| (binding.sym.as_str(), binding.binding_type))
            .collect();
        assert_eq!(
            bindings,
            [
                ("x", BindingTypes::SetupMaybeRef),
                ("y", BindingTypes::SetupMaybeRef),
            ]
        );

        let code: String = result.code.split_whitespace().collect();
        if is_prod {
            assert!(
                code.contains("_toDisplayString(_unref(y))"),
                "{}",
                result.code
            );
        } else {
            assert!(
                code.contains("_toDisplayString($setup.y)"),
                "{}",
                result.code
            );
            assert!(code.contains("return{x,y};"), "{}", result.code);
        }
        assert!(!code.contains("_ctx.y"), "{}", result.code);
    }
}

#[test]
fn mutable_rest_binding_can_be_reassigned_to_a_ref() {
    for kind in ["let", "var"] {
        let source = format!(
            r#"
<script setup>
import {{ ref }} from 'vue'
{kind} [...rest] = [1]
rest = ref([2])
</script>
<template>{{{{ rest }}}}</template>
"#
        );
        let result = compile_sfc(&source, true);
        let rest = result
            .setup_bindings
            .iter()
            .find(|binding| binding.sym == "rest")
            .expect("mutable rest binding should be recorded");
        assert_eq!(rest.binding_type, BindingTypes::SetupLet);
        let code: String = result.code.split_whitespace().collect();
        assert!(code.contains("rest=ref([2]);"), "{}", result.code);
        assert!(
            code.contains("_toDisplayString(_unref(rest))"),
            "{}",
            result.code
        );
    }
}

fn compile_sfc(source: &str, is_prod: bool) -> CompileResult {
    let result = compile(
        source,
        CompileOptions {
            filename: "RestBindings.vue".into(),
            id: "rest-bindings".into(),
            is_prod: Some(is_prod),
            is_custom_element: Some(false),
            ssr: Some(false),
            props_destructure: None,
            transform_asset_urls: None,
            gen_default_as: None,
            source_map: Some(false),
        },
    )
    .expect("rest binding fixture should compile");
    assert!(result.errors.is_empty(), "{:#?}", result.errors);
    result
}
