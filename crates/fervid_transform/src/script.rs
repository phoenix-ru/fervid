//! Responsible for `<script>` and `<script setup>` transformations and analysis.

use fervid_core::{SfcScriptBlock, VueImportsSet, TemplateGenerationMode};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        Function, Module, ObjectLit, ModuleItem,
    },
};

use crate::structs::{ScopeHelper, TransformScriptsResult};

use self::{
    options_api::{transform_and_record_script_options_api, AnalyzeOptions},
    setup::{transform_and_record_script_setup, merge_sfc_helper},
};

mod common;
mod options_api;
mod setup;
pub mod utils;

/// Transforms two script modules: `<script>` and `<script setup>`.
/// Returns a combined Module and a default export object.
///
/// Consumes both [`SfcScriptBlock`]s to avoid cloning.
///
/// It will populate the provided [`ScopeHelper`] with the analysis information, such as:
/// - Variable bindings (from `<script setup>` and from Options API);
/// - Import bindings;
/// - (TODO) Imported `.vue` component bindings;
pub fn transform_and_record_scripts(
    script_setup: Option<SfcScriptBlock>,
    script_legacy: Option<SfcScriptBlock>,
    scope_helper: &mut ScopeHelper,
) -> TransformScriptsResult {
    // Set inline flag in `ScopeHelper`
    if script_legacy.is_none() && script_setup.is_some() {
        scope_helper.template_generation_mode = TemplateGenerationMode::Inline;
    }

    //
    // STEP 1: Transform Options API `<script>`.
    //

    let mut module: Module = script_legacy.map_or_else(
        || Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        },
        |script| *script.content,
    );

    let script_options_transform_result =
        transform_and_record_script_options_api(&mut module, AnalyzeOptions::default());

    // Assign Options API bindings
    scope_helper.options_api_vars = Some(script_options_transform_result.vars);

    //
    // STEP 2: Prepare the exported object.
    //

    let mut export_obj = script_options_transform_result
        .default_export_obj
        .unwrap_or_else(|| ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        });

    //
    // STEP 3: Transform the Composition API `<script setup>`.
    //

    let mut setup_fn: Option<Box<Function>> = None;
    let mut added_imports = VueImportsSet::default();
    if let Some(script_setup) = script_setup {
        let setup_transform_result = transform_and_record_script_setup(script_setup, scope_helper);

        // TODO Push imports at module top or bottom? Or smart merge?
        // TODO Merge Vue imports produced by module transformation
        // TODO Is this really a `Vec<ModuleDecl>` or should rather be a `Vec<ModuleItem>`?
        for module_decl in setup_transform_result.module_decls.into_iter() {
            module.body.push(ModuleItem::ModuleDecl(module_decl));
        }

        // Merge fields into an SFC exported object
        added_imports = setup_transform_result.sfc_object_helper.vue_imports;
        merge_sfc_helper(setup_transform_result.sfc_object_helper, &mut export_obj.props);

        // TODO Adding bindings to `setup()` in Options API will get overwritten in `<script setup>`
        // https://play.vuejs.org/#eNp9U01v2zAM/SuELm6BNFmTm5F22IYetsM2bMUudTEYNp2okyVDklMPQf77SNpunS7txTQfH/n4Ye/Vh6aZ71pUqVpHrBuTR7zOLAB5IV4Urm7EFaAPw+5CV1eZir7FTA1RgMq5gbg4KnScGYyLKVGf0rb6ZBa7z/pDQ//rB2qA7cvs7ZJYaAL21CqnV6KKXS+2y4G1GljX/CB8NWqVekehynlK/g3awipTBBRtiK7mMbbucVJ3vaCEMZdHBJvXSAQ2pRAYPTFJL3F2pwm7nAGb5T1ZW2J3zsJGh0gF9nuJXcLhcDQr16OYa6J2NlB0kNC2aSPVr12JhhTE/soNnwzS+Lfh7qR9eA9JxC4mkEJSUtVERp3ujetg7Qi4o9PdC+BswfovmlmHwusmQsDY8uF03TgfgW/5iU4Jlaf1JXM5Ln92CScV1HmE25FzBQnBtDEpNS1L79hJwRKrvDUR9jysiJ2d9w6AJ9fb0YNxNynIBysgbUkesq1ePifddxNZNVMxUKjSm/lDcJZ+EKmYKf4mtUH/ra+bqXTUylRujHv8IhirzUa82GLx5wT+EDrGMvXdY0C/o2U/xWLuN0i35/DNz690okmQ7tkaYr8R/IHBmZZ77GkfW1tS2xOedPtZTqTt5jbcdBFtGIca13UQfqboXHyf10Z/bnc1X437VYd/HFh0XQ==
        setup_fn = setup_transform_result.setup_fn;
    }

    TransformScriptsResult {
        added_imports,
        module,
        export_obj,
        setup_fn,
    }
}
