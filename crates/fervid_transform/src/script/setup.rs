use fervid_core::SfcScriptBlock;
use swc_core::ecma::ast::{ModuleItem, ModuleDecl, PropOrSpread};

use crate::template::ScopeHelper;

pub struct ScriptSetupTransformResult {
    /// All the imports (and maybe exports) of the <script setup>
    pub decls: Vec<ModuleDecl>,
    /// Fields of the SFC object
    pub fields: Vec<PropOrSpread>
}

pub fn transform_and_record_script_setup(script_setup: SfcScriptBlock, scope_helper: &mut ScopeHelper) -> ScriptSetupTransformResult {
    let mut result = ScriptSetupTransformResult {
        decls: Vec::new(),
        fields: Vec::new(),
    };

    for module_item in script_setup.content.body {
        match module_item {
            ModuleItem::ModuleDecl(decl) => result.decls.push(decl),
            ModuleItem::Stmt(stmt) => {
                // todo actual analysis and transformation as in `fervid_script`
            }
        }
    }

    result
}
