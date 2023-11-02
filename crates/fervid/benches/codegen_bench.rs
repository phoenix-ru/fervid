use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use swc_core::common::DUMMY_SP;

mod fixtures;
use fixtures::FIXTURES;

fn codegen_benchmark(c: &mut Criterion) {
    for (name, component) in FIXTURES {
        c.bench_with_input(BenchmarkId::new("codegen: generate CSR+DEV", name), &component, |b, component| {
            let mut errors = Vec::new();
            let res = fervid_parser::parse_sfc(component, &mut errors);
            let sfc_blocks = res.unwrap();
            let mut template_block = sfc_blocks.template;
            let Some(ref mut template_block) = template_block else {
                panic!("Test component has no template block");
            };

            let mut bindings_helper = fervid_core::BindingsHelper::default();
            fervid_transform::template::transform_and_record_template(template_block, &mut bindings_helper);

            b.iter_batched(
                || template_block.clone(),
                |template_block| {
                    let mut ctx = fervid_codegen::CodegenContext::default();
                    let template_expr = ctx.generate_sfc_template(&template_block);
                    let script = swc_core::ecma::ast::Module { span: DUMMY_SP, body: vec![], shebang: None };
                    let sfc_export_obj = swc_core::ecma::ast::ObjectLit { span: DUMMY_SP, props: vec![] };
                    ctx.generate_module(Some(template_expr), script, sfc_export_obj, None);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, codegen_benchmark);
criterion_main!(benches);
