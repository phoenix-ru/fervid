//! Exports data structs used by the crate

use fervid_core::{BindingTypes, VueImportsSet};
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
    pub is_inline: bool,
    pub transform_mode: TemplateGenerationMode
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

#[derive(Debug, Default)]
pub enum TemplateGenerationMode {
    /// Applies the transformation as if the template is rendered inline
    /// and variables are directly accessible in the function scope.
    /// For example, if there is `const foo = ref(0)`, then `foo` will be transformed to `foo.value`.
    /// Non-ref bindings and literal constants will remain untouched.
    Inline,

    /// Applies the transformation as if the template is inside a
    /// `function render(_ctx, _cache, $props, $setup, $data, $options)`.\
    /// Variable access will be translated to object property access,
    /// e.g. `const foo = ref(0)` and `foo.bar` -> `$setup.foo.bar`.
    #[default]
    RenderFn
}

pub struct TransformScriptsResult {
    /// EcmaScript module
    pub module: Module,
    /// Default exported object (not linked to module yet)
    pub export_obj: ObjectLit,
    /// Setup function (not linked to default export yet)
    pub setup_fn: Option<Box<Function>>,
}
