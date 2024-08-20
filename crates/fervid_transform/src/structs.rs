//! Exports data structs used by the crate

use fervid_core::{
    BindingTypes, ComponentBinding, CustomDirectiveBinding, FervidAtom, SfcCustomBlock,
    SfcStyleBlock, SfcTemplateBlock, TemplateGenerationMode, VueImportsSet,
};
use fxhash::FxHashMap as HashMap;
use smallvec::SmallVec;
use swc_core::ecma::{
    ast::{Expr, ExprOrSpread, Function, Id, Module, ObjectLit, PropOrSpread},
    atoms::JsWord,
};

/// Context object. Currently very minimal but may grow over time.
pub struct TransformSfcContext {
    pub filename: String,
}

/// A helper which encompasses all the logic related to bindings,
/// such as their types, which of them were used, what components and directives
/// were seen in the template, etc.
#[derive(Debug, Default)]
pub struct BindingsHelper {
    /// All components present in the `<template>`
    pub components: HashMap<FervidAtom, ComponentBinding>,
    /// All custom directives present in the `<template>`
    pub custom_directives: HashMap<FervidAtom, CustomDirectiveBinding>,
    /// Are we compiling for DEV or PROD
    pub is_prod: bool,
    /// Is Typescript or Javascript used
    pub is_ts: bool,
    /// Scopes of the `<template>` for in-template variable resolutions
    pub template_scopes: Vec<TemplateScope>,
    /// Bindings in `<script setup>`
    pub setup_bindings: Vec<SetupBinding>,
    /// Bindings in `<script>`
    pub options_api_bindings: Option<Box<OptionsApiBindings>>,
    /// The mode with which `<template>` variables are resolved.
    /// Also controls in which mode should the template be generated:
    /// - inline as last statement of `setup` or
    /// - as a `render` function.
    pub template_generation_mode: TemplateGenerationMode,
    /// Identifiers used in the template and their respective binding types
    pub used_bindings: HashMap<FervidAtom, BindingTypes>,
    /// Imported symbols
    pub user_imports: HashMap<FervidAtom, ImportBinding>,
    /// Internal Vue imports used by built-in components, directives and others
    pub vue_imports: VueImportsSet,
    /// User imports from `vue` package
    pub vue_resolved_imports: Box<VueResolvedImports>,
}

// Todo maybe use SmallVec?
#[derive(Debug, Default, PartialEq)]
pub struct OptionsApiBindings {
    pub data: Vec<FervidAtom>,
    pub setup: Vec<SetupBinding>,
    pub props: Vec<FervidAtom>,
    pub inject: Vec<FervidAtom>,
    pub emits: Vec<FervidAtom>,
    pub components: Vec<FervidAtom>,
    pub computed: Vec<FervidAtom>,
    pub methods: Vec<FervidAtom>,
    pub expose: Vec<FervidAtom>,
    pub name: Option<FervidAtom>,
    pub directives: Vec<FervidAtom>,
    /// `SetupBinding` is used to distinguish between `.vue` and other imports
    pub imports: Vec<SetupBinding>,
}

/// Identifier plus a binding type
#[derive(Debug, PartialEq)]
pub struct SetupBinding(pub FervidAtom, pub BindingTypes);

#[derive(Debug)]
pub struct ImportBinding {
    /// Where it was imported from
    pub source: FervidAtom,
    /// What was imported
    pub imported: FervidAtom,
    /// As which variable was it imported
    pub local: FervidAtom,
    /// If it was imported in `<script setup>`
    pub is_from_setup: bool,
}

/// Template scope is for a proper handling of variables introduced in the template
/// by directives like `v-for` and `v-slot`
#[derive(Debug)]
pub struct TemplateScope {
    pub variables: SmallVec<[FervidAtom; 2]>,
    pub parent: u32,
}

/// Imports from "vue" package
#[derive(Debug, Default, PartialEq)]
pub struct VueResolvedImports {
    pub ref_import: Option<Id>,
    pub computed: Option<Id>,
    pub reactive: Option<Id>,
}

/// https://github.com/vuejs/rfcs/discussions/503
pub struct SfcDefineModel {
    pub name: JsWord,
    pub options: Option<Box<ExprOrSpread>>,
    pub local: bool,
}

#[derive(Default)]
pub struct SfcExportedObjectHelper {
    /// `emits` property
    pub emits: Option<Box<Expr>>,
    /// Should `async setup` be generated (when `await` was used)
    pub is_async_setup: bool,
    /// Whether `__emit` was referenced (e.g. as a result of `const foo = defineEmits()`)
    pub is_setup_emit_referenced: bool,
    /// Whether `__expose` was referenced (e.g. as a result of `defineExpose()`)
    pub is_setup_expose_referenced: bool,
    /// Whether `__props` was referenced (e.g. as a result of `const foo = defineProps()` or from `useModel`)
    pub is_setup_props_referenced: bool,
    /// To generate two-way binding code, as used in `defineModel`
    pub models: Vec<SfcDefineModel>,
    /// `props` property
    pub props: Option<Box<Expr>>,
    /// Other fields of the object
    pub untyped_fields: Vec<PropOrSpread>,
}

pub struct TransformScriptsResult {
    /// EcmaScript module
    pub module: Box<Module>,
    /// Default exported object (not linked to module yet)
    pub export_obj: ObjectLit,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
}

pub struct TransformSfcOptions<'s> {
    pub is_prod: bool,
    pub scope_id: &'s str,
    pub filename: &'s str,
}

pub struct TransformSfcResult {
    /// Helper with all the information about the bindings
    pub bindings_helper: BindingsHelper,
    /// Object exported from the `Module`, but detached from it
    pub exported_obj: ObjectLit,
    /// Module obtained by processing `<script>` and `<script setup>`
    pub module: Box<Module>,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
    /// Transformed template block
    pub template_block: Option<SfcTemplateBlock>,
    /// Transformed style blocks
    pub style_blocks: Vec<SfcStyleBlock>,
    /// Custom blocks
    pub custom_blocks: Vec<SfcCustomBlock>,
}

#[cfg(test)]
impl TransformSfcContext {
    pub fn anonymous() -> TransformSfcContext {
        TransformSfcContext {
            filename: "anonymous.vue".to_string(),
        }
    }
}
