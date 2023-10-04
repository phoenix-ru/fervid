mod attributes;
mod common;
mod error;
mod script;
mod sfc;
mod template;

pub use sfc::{parse_sfc, parse_html_document_fragment};

#[cfg(test)]
mod tests {
    use crate::sfc::parse_sfc;

    #[test]
    fn it_works() {
        let document = include_str!("../../fervid/benches/fixtures/input.vue");

        let mut errors = Vec::new();

        let now = std::time::Instant::now();
        let _ = parse_sfc(document, &mut errors);
        let elapsed = now.elapsed();
        println!("Elapsed: {:?}", elapsed);
        errors.clear();

        let now = std::time::Instant::now();
        let parsed = parse_sfc(document, &mut errors);
        let elapsed = now.elapsed();
        println!("Elapsed: {:?}", elapsed);

        println!("{:#?}", parsed);

        for error in errors {
            let e = error;
            println!(
                "{:?} {}",
                e.kind,
                &document[e.span.lo.0 as usize - 1..e.span.hi.0 as usize - 1]
            );
        }

        // assert_eq!(errors.len(), 0);
    }
}
