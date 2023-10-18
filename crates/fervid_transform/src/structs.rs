//! Exports data structs used by the crate

use fervid_core::{BindingTypes, VueImportsSet, FervidAtom, TemplateGenerationMode};
use fxhash::FxHashMap as HashMap;
use swc_core::ecma::{atoms::JsWord, ast::{Id, Expr, PropOrSpread, Module, ObjectLit, Function, ExprOrSpread}};
use smallvec::SmallVec;

#[derive(Debug, PartialEq)]
pub struct SetupBinding(pub JsWord, pub BindingTypes);

// Todo maybe use SmallVec?
#[derive(Debug, Default, PartialEq)]
pub struct ScriptLegacyVars {
    pub data: Vec<JsWord>,
    pub setup: Vec<SetupBinding>,
    pub props: Vec<JsWord>,
    pub inject: Vec<JsWord>,
    pub emits: Vec<JsWord>,
    pub components: Vec<JsWord>,
    pub computed: Vec<JsWord>,
    pub methods: Vec<JsWord>,
    pub expose: Vec<JsWord>,
    pub name: Option<JsWord>,
    pub directives: Vec<JsWord>,
    pub imports: Vec<Id>
}

/// Imports from "vue" package
#[derive(Debug, Default, PartialEq)]
pub struct VueResolvedImports {
    pub ref_import: Option<Id>,
    pub computed: Option<Id>,
    pub reactive: Option<Id>
}

#[derive(Debug)]
pub struct TemplateScope {
    pub variables: SmallVec<[JsWord; 1]>,
    pub parent: u32,
}

#[derive(Debug, Default)]
pub struct ScopeHelper {
    pub template_scopes: Vec<TemplateScope>,
    pub setup_bindings: Vec<SetupBinding>,
    pub options_api_vars: Option<Box<ScriptLegacyVars>>,
    pub template_generation_mode: TemplateGenerationMode,
    /// Identifiers used in the template and their respective binding types
    pub used_idents: HashMap<FervidAtom, BindingTypes>,
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
    /// What imports need to be used
    pub vue_imports: VueImportsSet
}

pub struct TransformScriptsResult {
    /// Imports added by transformation (usually by macros)
    pub added_imports: VueImportsSet,
    /// EcmaScript module
    pub module: Module,
    /// Default exported object (not linked to module yet)
    pub export_obj: ObjectLit,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
}
