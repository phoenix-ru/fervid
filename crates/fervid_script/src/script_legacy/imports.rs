use swc_core::ecma::ast::ImportDecl;

use crate::structs::ScriptLegacyVars;

pub fn collect_imports(import_decl: &ImportDecl, out: &mut ScriptLegacyVars) {
    todo!("Split imports between Vue and non-Vue")
}
