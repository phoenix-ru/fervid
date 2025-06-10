use std::{cell::RefCell, rc::Rc};

use error::TransformError;
use fervid_core::{SfcDescriptor, SfcScriptBlock, SfcScriptLang, TemplateGenerationMode};
use misc::infer_name;
use script::transform_and_record_scripts;
use style::{attach_scope_id, create_style_scope, transform_style_blocks};
use swc_core::ecma::ast::{ModuleDecl, ModuleItem};
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
pub fn transform_sfc(
    sfc_descriptor: SfcDescriptor,
    options: TransformSfcOptions<'_>,
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
        transform_and_record_template(&mut template, &mut ctx);
        if !template.roots.is_empty() {
            template_block = Some(template);
        }

        // Add transformed asset URL imports
        // TODO Move this to codegen?
        transform_result.module.body.extend(
            ctx.bindings_helper
                .imports
                .drain(..)
                .map(|v| ModuleItem::ModuleDecl(ModuleDecl::Import(v))),
        );
    }

    // Transform scoped CSS
    let mut style_blocks = sfc_descriptor.styles;
    let scope = create_style_scope(options.scope_id);
    let had_scoped_blocks = transform_style_blocks(&mut style_blocks, &scope, errors);
    if had_scoped_blocks {
        attach_scope_id(&mut transform_result, &scope);
    }

    // Augment with some metadata
    let mut exported_obj = transform_result.export_obj;
    infer_name(&mut exported_obj, options.filename);

    // Temp: extend errors
    errors.extend(ctx.errors);

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
    pub fn new(
        sfc_descriptor: &SfcDescriptor,
        options: &TransformSfcOptions,
    ) -> TransformSfcContext {
        // Create the bindings helper
        let mut bindings_helper = BindingsHelper {
            is_prod: options.is_prod,
            ..Default::default()
        };

        // TS if any of scripts is TS.
        // Unlike the official compiler, we don't care if languages are mixed, because nothing changes.
        let recognize_lang =
            |script: &SfcScriptBlock| matches!(script.lang, SfcScriptLang::Typescript);
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

        TransformSfcContext {
            filename: options.filename.to_string(),
            is_ce: options.is_ce,
            props_destructure: options.props_destructure,
            bindings_helper,
            deps: Default::default(),
            scopes: vec![],
            transform_asset_urls: options.transform_asset_urls.clone(),
            errors: vec![],
            warnings: vec![],
        }
    }

    pub fn root_scope(&mut self) -> TypeScopeContainer {
        if let Some(root_scope) = self.scopes.first() {
            return root_scope.clone();
        }

        let root_scope = Rc::new(RefCell::new(TypeScope::new(0, self.filename.to_owned())));
        self.scopes.push(root_scope.clone());

        root_scope
    }

    pub fn create_child_scope(&mut self, parent_scope: &TypeScope) -> TypeScopeContainer {
        let id = self.scopes.len();
        let child_scope = Rc::new(RefCell::new(TypeScope {
            id,
            // We unfortunately have to copy here
            filename: parent_scope.filename.to_owned(),
            imports: parent_scope.imports.to_owned(),
            types: parent_scope.types.to_owned(),
            declares: parent_scope.declares.to_owned(),
            is_generic_scope: false,
            exported_types: Default::default(),
            exported_declares: Default::default(),
        }));
        self.scopes.push(child_scope.clone());

        child_scope
    }

    #[inline]
    pub fn get_scope(&self, id: usize) -> Option<TypeScopeContainer> {
        self.scopes.get(id).cloned()
    }

    pub fn get_scope_or_root(&mut self, id: usize) -> TypeScopeContainer {
        self.get_scope(id).unwrap_or_else(|| self.root_scope())
    }
}
