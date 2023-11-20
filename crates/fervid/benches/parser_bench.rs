use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

mod fixtures;
use fixtures::FIXTURES;

fn parser_benchmark(c: &mut Criterion) {
    for input in FIXTURES {
        c.bench_with_input(
            BenchmarkId::new("parser: new parse", input.0),
            &input.1,
            |b, component| {
                let mut errors = Vec::new();
                b.iter(|| {
                    let mut parser = fervid_parser::SfcParser::new(black_box(&component), &mut errors);
                    let _ = parser.parse_sfc();
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
