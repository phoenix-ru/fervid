use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn swc_benchmark(c: &mut Criterion) {
    c.bench_function("swc transform", |b| {
        b.iter(|| parser::test_swc_transform(black_box("foo.bar.baz[test.keks]")))
    });
}

fn parser_benchmark(c: &mut Criterion) {
    let test_component = include_str!("../src/test/input.vue");

    c.bench_function("parser: parse", |b| {
        b.iter(|| parser::parse_sfc(black_box(test_component)))
    });
}

criterion_group!(benches, swc_benchmark, parser_benchmark);
criterion_main!(benches);
