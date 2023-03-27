use criterion::{criterion_group, criterion_main, Criterion};

fn codegen_benchmark(c: &mut Criterion) {
    let test_component = include_str!("../src/test/input.vue");

    c.bench_function("codegen: generate CSR+DEV", |b| {
        let ast = fervid::parse_sfc(test_component);
        let mut ast = &mut ast.unwrap().1;
        let ast = fervid::optimize_ast(&mut ast);

        b.iter_batched(
            || ast.clone(),
            |ast| fervid::compile_ast(ast, Default::default()),
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, codegen_benchmark);
criterion_main!(benches);
