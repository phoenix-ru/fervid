use fervid_core::SfcScriptBlock;
use swc_core::{ecma::ast::Module, common::DUMMY_SP};

use crate::template::ScopeHelper;

mod options;
mod setup;

pub fn transform_and_record_scripts(script_setup: Option<SfcScriptBlock>, script_legacy: Option<SfcScriptBlock>, scope_helper: &mut ScopeHelper) -> Module {
    let module_base = script_legacy.map_or_else(
        || Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        },
        |script| *script.content
    );

    // TODO

    module_base
}
