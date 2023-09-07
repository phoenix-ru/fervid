extern crate swc_core;
extern crate swc_ecma_codegen;
extern crate swc_ecma_parser;
use std::time::Instant;

use fervid::compile_sync_naive;

fn main() {
    let n = Instant::now();
    test_real_compilation();
    println!("Time took: {:?}", n.elapsed());
}

fn test_real_compilation() {
    let test = include_str!("../benches/fixtures/input.vue");

    let compiled_code = match compile_sync_naive(test) {
        Ok(result) => result,
        Err(e) => std::panic::panic_any(e)
    };

    #[cfg(feature = "dbg_print")]
    {
        println!("Result: {:#?}", ast);
        println!("Remaining: {:?}", res.0);

        println!();
        println!("SFC blocks length: {}", ast.len());

        println!();
        println!("Scopes: {:#?}", scope_helper);
    }

    // Real codegen
    println!("\n[Real File Compile Result]");
    println!("{compiled_code}");
    // println!("{}", compile_sfc(sfc_blocks, scope_helper).unwrap());
}
