use std::path::PathBuf;

use swc_core::{
    common::{
        errors::HANDLER, sync::Lrc, FilePathMapping, Globals, Mark, SourceMap, SyntaxContext,
        GLOBALS,
    },
    ecma::{
        ast::{EsVersion, Program},
        visit::VisitMutWith,
    },
};
use swc_ecma_lints::{self, rule::Rule, rules::LintParams};
use swc_ecma_transforms_base::resolver;
use swc_error_reporters::handler::{try_with_handler, HandlerOpts};

pub fn lint(input: &str) {
    let mut parse_errors = Vec::new();
    let mut sfc_parser = fervid_parser::SfcParser::new(input, &mut parse_errors);

    let Ok(sfc_descriptor) = sfc_parser.parse_sfc() else {
        println!("No descriptor");
        return;
    };

    let Some(script_setup) = sfc_descriptor.script_setup else {
        println!("No setup");
        return;
    };

    let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
    cm.new_source_file(
        Lrc::new(swc_core::common::FileName::Real(PathBuf::from("input.vue"))),
        input.to_owned(),
    );
    let handler_opts = HandlerOpts::default();
    let result = GLOBALS.set(&Globals::new(), || {
        try_with_handler(cm.clone(), handler_opts, |handler| {
            HANDLER.set(handler, || {
                let module = script_setup.content;
                let mut program = Program::Module(*module);

                let unresolved_mark = Mark::new();
                let top_level_mark = Mark::new();
                let unresolved_ctxt = SyntaxContext::empty().apply_mark(unresolved_mark);
                let top_level_ctxt = SyntaxContext::empty().apply_mark(top_level_mark);

                program.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));

                let mut rules = swc_ecma_lints::rules::all(LintParams {
                    program: &program,
                    lint_config: &Default::default(),
                    unresolved_ctxt,
                    top_level_ctxt,
                    es_version: EsVersion::latest(),
                    source_map: cm.clone(),
                });

                let module = program.expect_module();
                rules.lint_module(&module);
                Ok(())
            })
        })
    });

    // eprintln!("{}", result.unwrap_err());

    assert!(result.is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        lint(include_str!("../../fervid/benches/fixtures/input.vue"));
    }
}
