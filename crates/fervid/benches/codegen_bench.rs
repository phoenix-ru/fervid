use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use fervid::PropsDestructureConfig;
use fervid_transform::{BindingsHelper, TransformAssetUrlsConfig, TransformSfcContext};
use swc_core::common::DUMMY_SP;

mod fixtures;
use fixtures::FIXTURES;

fn codegen_benchmark(c: &mut Criterion) {
    for (name, component) in FIXTURES {
        c.bench_with_input(BenchmarkId::new("codegen: generate CSR+DEV", name), &component, |b, component| {
            let mut errors = Vec::new();
            let mut parser = fervid_parser::SfcParser::new(&component, &mut errors);
            let res = parser.parse_sfc();
            let sfc_blocks = res.unwrap();
            let mut template_block = sfc_blocks.template;
            let Some(ref mut template_block) = template_block else {
                panic!("Test component has no template block");
            };

            // Copy of `TransformSfcContext::anonymous` because it is a test-only function
            let filename = "anonymous.vue".to_string();
            let mut ctx = TransformSfcContext {
                filename: filename.to_owned(),
                bindings_helper: BindingsHelper::default(),
                is_ce: false,
                props_destructure: PropsDestructureConfig::default(),
                deps: Default::default(),
                scopes: vec![],
                transform_asset_urls: TransformAssetUrlsConfig::default(),
            };

            fervid_transform::template::transform_and_record_template(template_block, &mut ctx);

            b.iter_batched(
                || template_block.clone(),
                |template_block| {
                    let mut ctx = fervid_codegen::CodegenContext::default();
                    let template_expr = ctx.generate_sfc_template(&template_block);
                    let script = swc_core::ecma::ast::Module { span: DUMMY_SP, body: vec![], shebang: None };
                    let sfc_export_obj = swc_core::ecma::ast::ObjectLit { span: DUMMY_SP, props: vec![] };
                    ctx.generate_module(template_expr, script, sfc_export_obj, None, None);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, codegen_benchmark);
criterion_main!(benches);
