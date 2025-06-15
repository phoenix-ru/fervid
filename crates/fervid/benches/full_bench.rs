use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

mod fixtures;
use fixtures::FIXTURES;

fn full_compile_benchmark(c: &mut Criterion) {
    for (name, component) in FIXTURES {
        c.bench_with_input(
            BenchmarkId::new("compile_sync_naive", name),
            &component,
            |b, component| {
                b.iter_batched(
                    || (),
                    |_| {
                        let _ = fervid::compile_sync_naive(component, true);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
}

criterion_group!(benches, full_compile_benchmark);
criterion_main!(benches);
