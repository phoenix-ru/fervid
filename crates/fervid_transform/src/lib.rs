use std::{cell::RefCell, rc::Rc};

use error::TransformError;
use fervid_core::{SfcDescriptor, SfcScriptBlock, SfcScriptLang, TemplateGenerationMode};
use misc::infer_name;
use script::transform_and_record_scripts;
use style::{attach_scope_id, create_style_scope, transform_style_blocks};
use template::transform_and_record_template;

#[macro_use]
extern crate lazy_static;

pub mod atoms;
pub mod error;
pub mod misc;
pub mod script;
pub mod structs;
pub mod style;
pub mod template;

#[cfg(test)]
mod test_utils;

pub use structs::*;

/// Applies all the necessary transformations to the SFC.
///
/// The transformations can be fine-tuned by using individual `transform_` functions.
pub fn transform_sfc<'o>(
    sfc_descriptor: SfcDescriptor,
    options: TransformSfcOptions<'o>,
    errors: &mut Vec<TransformError>,
) -> TransformSfcResult {
    // Create the context
    let mut ctx = TransformSfcContext::new(&sfc_descriptor, &options);

    // Transform the scripts
    let mut transform_result = transform_and_record_scripts(
        &mut ctx,
        sfc_descriptor.script_setup,
        sfc_descriptor.script_legacy,
        errors,
    );

    // Transform the template if it is present
    let mut template_block = None;
    if let Some(mut template) = sfc_descriptor.template {
        transform_and_record_template(&mut template, &mut ctx.bindings_helper);
        if !template.roots.is_empty() {
            template_block = Some(template);
        }
    }

    // Transform scoped CSS
    let mut style_blocks = sfc_descriptor.styles;
    let scope = create_style_scope(&options.scope_id);
    let had_scoped_blocks = transform_style_blocks(&mut style_blocks, &scope, errors);
    if had_scoped_blocks {
        attach_scope_id(&mut transform_result, &scope);
    }

    // Augment with some metadata
    let mut exported_obj = transform_result.export_obj;
    infer_name(&mut exported_obj, &options.filename);

    TransformSfcResult {
        bindings_helper: ctx.bindings_helper,
        exported_obj,
        module: transform_result.module,
        setup_fn: transform_result.setup_fn,
        template_block,
        style_blocks,
        custom_blocks: sfc_descriptor.custom_blocks,
    }
}

impl TransformSfcContext {
    pub fn new(sfc_descriptor: &SfcDescriptor, options: &TransformSfcOptions) -> TransformSfcContext {
        // Create the bindings helper
        let mut bindings_helper = BindingsHelper::default();
        bindings_helper.is_prod = options.is_prod;

        // TS if any of scripts is TS.
        // Unlike the official compiler, we don't care if languages are mixed, because nothing changes.
        let recognize_lang = |script: &SfcScriptBlock| matches!(script.lang, SfcScriptLang::Typescript);
        bindings_helper.is_ts = sfc_descriptor
            .script_setup
            .as_ref()
            .map_or(false, recognize_lang)
            || sfc_descriptor
                .script_legacy
                .as_ref()
                .map_or(false, recognize_lang);

        // Set inline flag in `BindingsHelper`
        if bindings_helper.is_prod && sfc_descriptor.script_setup.is_some() {
            bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
        }

        let scope = Rc::new(RefCell::from(TypeScope::new(options.filename.to_string())));

        TransformSfcContext {
            filename: options.filename.to_string(),
            is_ce: false, // todo
            bindings_helper,
            scope,
        }
    }
}
