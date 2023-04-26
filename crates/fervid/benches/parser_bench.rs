use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn swc_benchmark(c: &mut Criterion) {
    c.bench_function("swc transform", |b| {
        b.iter(|| fervid::test_swc_transform(black_box("foo.bar.baz[test.keks]")))
    });
}

fn parser_benchmark(c: &mut Criterion) {
    let inputs = vec![
        ("input.vue", include_str!("./fixtures/input.vue")),
        ("ElTable.vue", include_str!("./fixtures/ElTable.vue"))
    ];

    for input in inputs {
        c.bench_with_input(
            BenchmarkId::new("parser: parse", input.0),
            &input.1,
            |b, component| b.iter(|| fervid::parse_sfc(black_box(component))),
        );
    }
}

criterion_group!(benches, swc_benchmark, parser_benchmark);
criterion_main!(benches);
