use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use swc_core::common::DUMMY_SP;

fn codegen_benchmark(c: &mut Criterion) {
    let inputs = vec![
        ("input.vue", include_str!("./fixtures/input.vue")),
        ("ElTable.vue", include_str!("./fixtures/ElTable.vue")),
        ("TodoApp.vue", include_str!("./fixtures/TodoApp.vue")),
    ];

    for (name, component) in inputs {
        c.bench_with_input(BenchmarkId::new("codegen: generate CSR+DEV", name), &component, |b, component| {
            let res = fervid::parse_sfc(component);
            let sfc_blocks = &mut res.unwrap().1;
            let template_block = sfc_blocks.iter_mut().find_map(|block| match block {
                fervid::SfcBlock::Template(template_block) => Some(template_block),
                _ => None,
            });
            let Some(template_block) = template_block else {
                panic!("Test component has no template block");
            };

            let mut scope_helper = fervid_transform::template::ScopeHelper::default();
            fervid_transform::template::transform_and_record_template(template_block, &mut scope_helper);

            b.iter_batched(
                || template_block.clone(),
                |template_block| {
                    let mut ctx = fervid_codegen::CodegenContext::default();
                    let template_expr = ctx.generate_sfc_template(&template_block);
                    let script = swc_core::ecma::ast::Module { span: DUMMY_SP, body: vec![], shebang: None };
                    ctx.generate_module(template_expr, script);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, codegen_benchmark);
criterion_main!(benches);
