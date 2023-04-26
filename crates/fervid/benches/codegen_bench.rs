use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn codegen_benchmark(c: &mut Criterion) {
    let inputs = vec![
        ("input.vue", include_str!("./fixtures/input.vue")),
        ("ElTable.vue", include_str!("./fixtures/ElTable.vue"))
    ];

    for (name, component) in inputs {
        c.bench_with_input(BenchmarkId::new("codegen: generate CSR+DEV", name), &component, |b, component| {
            let res = fervid::parse_sfc(component);
            let sfc_blocks = &mut res.unwrap().1;
            let template_block = sfc_blocks.iter_mut().find_map(|block| match block {
                fervid::SfcBlock::Template(template_block) => Some(template_block),
                _ => None,
            });
            let Some(mut template_block) = template_block else {
                panic!("Test component has no template block");
            };
    
            fervid::analyzer::ast_optimizer::optimize_template(&mut template_block);
    
            b.iter_batched(
                || sfc_blocks.clone(),
                |blocks| fervid::compile_sfc(blocks, Default::default()),
                criterion::BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, codegen_benchmark);
criterion_main!(benches);
