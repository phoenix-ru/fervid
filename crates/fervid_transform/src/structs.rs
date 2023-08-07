//! Exports data structs used by the crate

use fervid_core::BindingTypes;
use swc_core::ecma::{atoms::JsWord, ast::Id};
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
