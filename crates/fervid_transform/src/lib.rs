use fervid_core::{SfcDescriptor, SfcTemplateBlock, VueImportsSet, BindingTypes, FervidAtom, TemplateGenerationMode};
use fxhash::FxHashMap as HashMap;
use script::transform_and_record_scripts;
use structs::BindingsHelper;
use swc_core::ecma::ast::{Module, ObjectLit, Function};
use template::transform_and_record_template;

#[macro_use]
extern crate lazy_static;

pub mod atoms;
pub mod script;
pub mod structs;
pub mod template;

#[cfg(test)]
mod test_utils;

pub struct TransformSfcResult {
    /// Object exported from the `Module`, but detached from it
    pub exported_obj: ObjectLit,
    /// Module obtained by processing `<script>` and `<script setup>`
    pub module: Module,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
    /// Transformed template block
    pub template_block: Option<SfcTemplateBlock>,
    /// All imports `from 'vue'`
    pub used_vue_imports: VueImportsSet,
    /// Identifiers used in the template and their respective binding types
    pub used_idents: HashMap<FervidAtom, BindingTypes>,
    /// In which mode should the template be generated:
    /// - inline as last statement of `setup` or
    /// - as a `render` function.
    pub template_generation_mode: TemplateGenerationMode
}

/// Applies all the necessary transformations to the SFC.
///
/// The transformations can be fine-tuned by using individual `transform_` functions.
pub fn transform_sfc(sfc_descriptor: SfcDescriptor, is_prod: bool) -> TransformSfcResult {
    let mut template_block = None;

    let mut bindings_helper = BindingsHelper::default();
    bindings_helper.is_prod = is_prod;
    let transform_result = transform_and_record_scripts(
        sfc_descriptor.script_setup,
        sfc_descriptor.script_legacy,
        &mut bindings_helper,
    );

    if let Some(mut template) = sfc_descriptor.template {
        transform_and_record_template(&mut template, &mut bindings_helper);
        if !template.roots.is_empty() {
            template_block = Some(template);
        }
    }

    TransformSfcResult {
        exported_obj: transform_result.export_obj,
        module: transform_result.module,
        setup_fn: transform_result.setup_fn,
        template_block,
        template_generation_mode: bindings_helper.template_generation_mode,
        used_idents: bindings_helper.used_bindings,
        used_vue_imports: bindings_helper.vue_imports,
    }
}
