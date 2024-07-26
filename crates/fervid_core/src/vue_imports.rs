use crate::FervidAtom;
use flagset::{flags, FlagSet};
use strum_macros::{AsRefStr, EnumString, IntoStaticStr};

flags! {
    #[derive(AsRefStr, EnumString, IntoStaticStr)]
    pub enum VueImports: u64 {
        #[strum(serialize = "_createBlock")]
        CreateBlock,
        #[strum(serialize = "_createCommentVNode")]
        CreateCommentVNode,
        #[strum(serialize = "_createElementBlock")]
        CreateElementBlock,
        #[strum(serialize = "_createElementVNode")]
        CreateElementVNode,
        #[strum(serialize = "_createTextVNode")]
        CreateTextVNode,
        #[strum(serialize = "_createVNode")]
        CreateVNode,
        #[strum(serialize = "_defineComponent")]
        DefineComponent,
        #[strum(serialize = "_Fragment")]
        Fragment,
        #[strum(serialize = "_isMemoSame")]
        IsMemoSame,
        #[strum(serialize = "_isRef")]
        IsRef,
        #[strum(serialize = "_KeepAlive")]
        KeepAlive,
        #[strum(serialize = "_mergeModels")]
        MergeModels,
        #[strum(serialize = "_normalizeClass")]
        NormalizeClass,
        #[strum(serialize = "_normalizeStyle")]
        NormalizeStyle,
        #[strum(serialize = "_openBlock")]
        OpenBlock,
        #[strum(serialize = "_renderList")]
        RenderList,
        #[strum(serialize = "_renderSlot")]
        RenderSlot,
        #[strum(serialize = "_resolveComponent")]
        ResolveComponent,
        #[strum(serialize = "_resolveDirective")]
        ResolveDirective,
        #[strum(serialize = "_resolveDynamicComponent")]
        ResolveDynamicComponent,
        #[strum(serialize = "_setBlockTracking")]
        SetBlockTracking,
        #[strum(serialize = "_Suspense")]
        Suspense,
        #[strum(serialize = "_Teleport")]
        Teleport,
        #[strum(serialize = "_toDisplayString")]
        ToDisplayString,
        #[strum(serialize = "_Transition")]
        Transition,
        #[strum(serialize = "_TransitionGroup")]
        TransitionGroup,
        #[strum(serialize = "_unref")]
        Unref,
        #[strum(serialize = "_useModel")]
        UseModel,
        #[strum(serialize = "_useSlots")]
        UseSlots,
        #[strum(serialize = "_vModelCheckbox")]
        VModelCheckbox,
        #[strum(serialize = "_vModelDynamic")]
        VModelDynamic,
        #[strum(serialize = "_vModelRadio")]
        VModelRadio,
        #[strum(serialize = "_vModelSelect")]
        VModelSelect,
        #[strum(serialize = "_vModelText")]
        VModelText,
        #[strum(serialize = "_vShow")]
        VShow,
        #[strum(serialize = "_withCtx")]
        WithCtx,
        #[strum(serialize = "_withDirectives")]
        WithDirectives,
        #[strum(serialize = "_withMemo")]
        WithMemo,
        #[strum(serialize = "_withModifiers")]
        WithModifiers,
    }
}

impl VueImports {
    #[inline]
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    #[inline]
    pub fn as_atom(self) -> FervidAtom {
        self.as_str().into()
    }
}

pub type VueImportsSet = FlagSet<VueImports>;
