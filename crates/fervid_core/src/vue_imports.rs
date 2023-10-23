use flagset::{flags, FlagSet};

use crate::FervidAtom;

flags! {
    // #[derive(Clone, Copy)]
    pub enum VueImports: u64 {
        CreateBlock,
        CreateCommentVNode,
        CreateElementBlock,
        CreateElementVNode,
        CreateTextVNode,
        CreateVNode,
        Fragment,
        KeepAlive,
        MergeModels,
        NormalizeClass,
        NormalizeStyle,
        OpenBlock,
        RenderList,
        RenderSlot,
        ResolveComponent,
        ResolveDirective,
        ResolveDynamicComponent,
        Suspense,
        Teleport,
        ToDisplayString,
        Transition,
        TransitionGroup,
        Unref,
        UseModel,
        VModelCheckbox,
        VModelDynamic,
        VModelRadio,
        VModelSelect,
        VModelText,
        VShow,
        WithCtx,
        WithDirectives,
        WithModifiers,
    }
}

impl VueImports {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            VueImports::CreateBlock => "_createBlock",
            VueImports::CreateCommentVNode => "_createCommentVNode",
            VueImports::CreateElementBlock => "_createElementBlock",
            VueImports::CreateElementVNode => "_createElementVNode",
            VueImports::CreateTextVNode => "_createTextVNode",
            VueImports::CreateVNode => "_createVNode",
            VueImports::Fragment => "_Fragment",
            VueImports::KeepAlive => "_KeepAlive",
            VueImports::MergeModels => "_mergeModels",
            VueImports::NormalizeClass => "_normalizeClass",
            VueImports::NormalizeStyle => "_normalizeStyle",
            VueImports::OpenBlock => "_openBlock",
            VueImports::RenderList => "_renderList",
            VueImports::RenderSlot => "_renderSlot",
            VueImports::ResolveComponent => "_resolveComponent",
            VueImports::ResolveDirective => "_resolveDirective",
            VueImports::ResolveDynamicComponent => "_resolveDynamicComponent",
            VueImports::Suspense => "_Suspense",
            VueImports::Teleport => "_Teleport",
            VueImports::ToDisplayString => "_toDisplayString",
            VueImports::Transition => "_Transition",
            VueImports::TransitionGroup => "_TransitionGroup",
            VueImports::Unref => "_unref",
            VueImports::UseModel => "_useModel",
            VueImports::VModelCheckbox => "_vModelCheckbox",
            VueImports::VModelDynamic => "_vModelDynamic",
            VueImports::VModelRadio => "_vModelRadio",
            VueImports::VModelSelect => "_vModelSelect",
            VueImports::VModelText => "_vModelText",
            VueImports::VShow => "_vShow",
            VueImports::WithCtx => "_withCtx",
            VueImports::WithDirectives => "_withDirectives",
            VueImports::WithModifiers => "_withModifiers",
        }
    }

    #[inline]
    pub fn as_atom(self) -> FervidAtom {
        self.as_str().into()
    }
}

pub type VueImportsSet = FlagSet<VueImports>;
