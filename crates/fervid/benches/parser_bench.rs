use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn parser_benchmark(c: &mut Criterion) {
    let inputs = vec![
        ("input.vue", include_str!("./fixtures/input.vue")),
        ("ElTable.vue", include_str!("./fixtures/ElTable.vue")),
        ("TodoApp.vue", include_str!("./fixtures/TodoApp.vue")),
    ];

    for input in inputs {
        c.bench_with_input(
            BenchmarkId::new("parser: new parse", input.0),
            &input.1,
            |b, component| {
                let mut errors = Vec::new();
                b.iter(|| {
                    let _ = fervid_parser::parse_sfc(black_box(component), &mut errors);
                    errors.clear();
                })
            },
        );

        c.bench_with_input(
            BenchmarkId::new("parser: parse", input.0),
            &input.1,
            |b, component| b.iter(|| fervid::parser::core::parse_sfc(black_box(component))),
        );
    }
}

criterion_group!(benches, parser_benchmark);
criterion_main!(benches);
