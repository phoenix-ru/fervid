//! Exports data structs used by the crate

use std::{cell::RefCell, rc::Rc};

use fervid_core::{
    BindingTypes, ComponentBinding, CustomDirectiveBinding, FervidAtom, SfcCustomBlock,
    SfcStyleBlock, SfcTemplateBlock, TemplateGenerationMode, VueImportsSet,
};
use fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use smallvec::SmallVec;
use swc_core::{
    common::{Span, DUMMY_SP},
    ecma::ast::{
        Decl, Expr, ExprOrSpread, Function, Id, Module, ObjectLit, PropOrSpread, Str, TsType,
    },
};

/// Context object. Currently very minimal but may grow over time.
pub struct TransformSfcContext {
    pub filename: String,
    // pub is_prod: bool, // This is a part of BindingsHelper
    /// Enable/disable the props destructure, or error when usage is encountered
    pub props_destructure: PropsDestructureConfig,
    /// For Custom Elements
    pub is_ce: bool,
    pub bindings_helper: BindingsHelper,
    pub deps: HashSet<String>,
    pub(crate) scopes: Vec<TypeScopeContainer>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum PropsDestructureConfig {
    #[default]
    False,
    True,
    Error,
}

#[derive(Debug)]
pub struct PropsDestructureBinding {
    pub local: FervidAtom,
    pub default: Option<Box<Expr>>,
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
    /// Used for props destructure
    pub props_aliases: HashMap<FervidAtom, FervidAtom>,
    /// Bindings collected from the props destructure variable declaration
    pub props_destructured_bindings: HashMap<FervidAtom, PropsDestructureBinding>,
    /// Used for props destructure to store `rest` of `const { foo, bar, ...rest } = defineProps()`
    pub props_destructured_rest_id: Option<FervidAtom>,
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

#[derive(Debug, Clone)]
pub struct ScopeTypeNode {
    pub value: TypeOrDecl,
    pub owner_scope: usize,
    pub namespace: Option<Rc<RefCell<Decl>>>,
}

#[derive(Debug, Clone)]
pub enum TypeOrDecl {
    Type(Rc<TsType>),
    Decl(Rc<RefCell<Decl>>),
}

#[derive(Debug)]
pub struct TypeScope {
    pub id: usize,
    pub filename: String,
    // source: String,
    // offset: usize,
    pub imports: HashMap<FervidAtom, ImportBinding>,
    pub types: HashMap<FervidAtom, ScopeTypeNode>,
    pub declares: HashMap<FervidAtom, ScopeTypeNode>,
    pub is_generic_scope: bool,
    // resolved_import_sources: HashMap<FervidAtom, String>,
    pub exported_types: HashMap<FervidAtom, ScopeTypeNode>,
    pub exported_declares: HashMap<FervidAtom, ScopeTypeNode>,
}

/// Container for easy sharing and modification of scopes
pub type TypeScopeContainer = Rc<RefCell<TypeScope>>;

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
pub struct SetupBinding {
    pub sym: FervidAtom,
    pub binding_type: BindingTypes,
    pub span: Span,
}

#[derive(Debug, Clone)]
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
    pub name: Str,
    pub prop_options: Option<Box<Expr>>,
    pub use_model_options: Option<Box<ExprOrSpread>>,
    pub ts_type: Option<TsType>,
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
    /// Whether `defineOptions` was already used
    pub has_define_options: bool,
    /// Whether `defineSlots` was already used
    pub has_define_slots: bool,
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
    pub is_ce: bool,
    pub props_destructure: PropsDestructureConfig,
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

impl SetupBinding {
    pub fn new(sym: FervidAtom, binding_type: BindingTypes) -> SetupBinding {
        SetupBinding {
            sym,
            binding_type,
            span: DUMMY_SP,
        }
    }

    pub fn new_spanned(sym: FervidAtom, binding_type: BindingTypes, span: Span) -> SetupBinding {
        SetupBinding {
            sym,
            binding_type,
            span,
        }
    }
}

#[cfg(test)]
impl TransformSfcContext {
    pub fn anonymous() -> TransformSfcContext {
        let filename = "anonymous.vue".to_string();
        TransformSfcContext {
            filename: filename.to_owned(),
            bindings_helper: BindingsHelper::default(),
            is_ce: false,
            props_destructure: PropsDestructureConfig::default(),
            deps: HashSet::default(),
            scopes: vec![],
        }
    }
}

impl TypeScope {
    pub fn new(id: usize, filename: String) -> TypeScope {
        TypeScope {
            id,
            filename,
            imports: Default::default(),
            types: Default::default(),
            declares: Default::default(),
            is_generic_scope: false,
            exported_types: Default::default(),
            exported_declares: Default::default(),
        }
    }
}

impl ScopeTypeNode {
    pub fn new(value: TypeOrDecl) -> ScopeTypeNode {
        ScopeTypeNode {
            value,
            owner_scope: 0,
            namespace: None,
        }
    }

    pub fn from_decl(decl: Decl) -> ScopeTypeNode {
        ScopeTypeNode::new(TypeOrDecl::Decl(Rc::new(decl.into())))
    }

    pub fn from_type(ts_type: TsType) -> ScopeTypeNode {
        ScopeTypeNode::new(TypeOrDecl::Type(Rc::new(ts_type)))
    }
}
