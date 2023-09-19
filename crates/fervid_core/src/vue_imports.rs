use flagset::{flags, FlagSet};

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

pub type VueImportsSet = FlagSet<VueImports>;
