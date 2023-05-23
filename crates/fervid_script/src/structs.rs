use swc_core::ecma::{atoms::JsWord, ast::Id};

/// https://github.com/vuejs/core/blob/020851e57d9a9f727c6ea07e9c1575430af02b73/packages/compiler-core/src/options.ts#L76
#[derive(Debug, PartialEq)]
pub enum BindingTypes {
    /// returned from data()
    Data,
    /// declared as a prop
    Props,
    /// a local alias of a `<script setup>` destructured prop.
    /// the original is stored in __propsAliases of the bindingMetadata object.
    PropsAliased,
    /// a let binding (may or may not be a ref)
    SetupLet,
    /// a const binding that can never be a ref.
    /// these bindings don't need `unref()` calls when processed in inlined
    /// template expressions.
    SetupConst,
    /// a const binding that does not need `unref()`, but may be mutated.
    SetupReactiveConst,
    /// a const binding that may be a ref
    SetupMaybeRef,
    /// bindings that are guaranteed to be refs
    SetupRef,
    /// declared by other options, e.g. computed, inject
    Options,
    /// a literal constant, e.g. 'foo', 1, true
    LiteralConst
}

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
#[derive(Debug, Default)]
pub struct VueResolvedImports {
    pub ref_import: Option<Id>,
    pub computed: Option<Id>,
    pub reactive: Option<Id>
}
