//! Exports data structs used by the crate

use fervid_core::{BindingsHelper, SfcCustomBlock, SfcStyleBlock, SfcTemplateBlock};
use swc_core::ecma::{atoms::JsWord, ast::{Id, Expr, PropOrSpread, Module, ObjectLit, Function, ExprOrSpread}};

/// Imports from "vue" package
#[derive(Debug, Default, PartialEq)]
pub struct VueResolvedImports {
    pub ref_import: Option<Id>,
    pub computed: Option<Id>,
    pub reactive: Option<Id>
}

/// https://github.com/vuejs/rfcs/discussions/503
pub struct SfcDefineModel {
    pub name: JsWord,
    pub options: Option<Box<ExprOrSpread>>,
    pub local: bool
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
    pub module: Module,
    /// Default exported object (not linked to module yet)
    pub export_obj: ObjectLit,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
}

pub struct TransformSfcOptions<'s> {
    pub is_prod: bool,
    pub scope_id: &'s str,
    pub filename: &'s str
}

pub struct TransformSfcResult {
    /// Helper with all the information about the bindings
    pub bindings_helper: BindingsHelper,
    /// Object exported from the `Module`, but detached from it
    pub exported_obj: ObjectLit,
    /// Module obtained by processing `<script>` and `<script setup>`
    pub module: Module,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
    /// Transformed template block
    pub template_block: Option<SfcTemplateBlock>,
    /// Transformed style blocks
    pub style_blocks: Vec<SfcStyleBlock>,
    /// Custom blocks
    pub custom_blocks: Vec<SfcCustomBlock>,
}
