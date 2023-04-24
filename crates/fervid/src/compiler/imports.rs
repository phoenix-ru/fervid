use super::codegen::CodegenContext;

// TODO refactor to a macro
// This is annoying me a lot

static CREATE_BLOCK: &str = "_createBlock";
static CREATE_COMMENT_VNODE: &str = "_createCommentVNode";
static CREATE_ELEMENT_BLOCK: &str = "_createElementBlock";
static CREATE_ELEMENT_VNODE: &str = "_createElementVNode";
static CREATE_TEXT_VNODE: &str = "_createTextVNode";
static CREATE_VNODE: &str = "_createVNode";
static FRAGMENT: &str = "_Fragment";
static KEEP_ALIVE: &str = "_KeepAlive";
static NORMALIZE_CLASS: &str = "_normalizeClass";
static NORMALIZE_STYLE: &str = "_normalizeStyle";
static OPEN_BLOCK: &str = "_openBlock";
static RENDER_LIST: &str = "_renderList";
static RENDER_SLOT: &str = "_renderSlot";
static RESOLVE_COMPONENT: &str = "_resolveComponent";
static RESOLVE_DIRECTIVE: &str = "_resolveDirective";
static RESOLVE_DYNAMIC_COMPONENT: &str = "_resolveDynamicComponent";
static SUSPENSE: &str = "_Suspense";
static TELEPORT: &str = "_Teleport";
static TO_DISPLAY_STRING: &str = "_toDisplayString";
static TRANSITION: &str = "_Transition";
static TRANSITION_GROUP: &str = "_TransitionGroup";
static VMODEL_CHECKBOX: &str = "_vModelCheckbox";
static VMODEL_RADIO: &str = "_vModelRadio";
static VMODEL_SELECT: &str = "_vModelSelect";
static VMODEL_TEXT: &str = "_vModelText";
static VSHOW: &str = "_vShow";
static WITH_CTX: &str = "_withCtx";
static WITH_DIRECTIVES: &str = "_withDirectives";
static WITH_MODIFIERS: &str = "_withModifiers";

#[derive(Clone, Copy)]
pub enum VueImports {
  CreateBlock,
  CreateCommentVNode,
  CreateElementBlock,
  CreateElementVNode,
  CreateTextVNode,
  CreateVNode,
  Fragment,
  KeepAlive,
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
  VModelCheckbox,
  VModelRadio,
  VModelSelect,
  VModelText,
  VShow,
  WithCtx,
  WithDirectives,
  WithModifiers
}

static ALL_IMPORTS: [VueImports; 29] = [
  VueImports::CreateBlock,
  VueImports::CreateCommentVNode,
  VueImports::CreateElementBlock,
  VueImports::CreateElementVNode,
  VueImports::CreateTextVNode,
  VueImports::CreateVNode,
  VueImports::Fragment,
  VueImports::KeepAlive,
  VueImports::NormalizeClass,
  VueImports::NormalizeStyle,
  VueImports::OpenBlock,
  VueImports::RenderList,
  VueImports::RenderSlot,
  VueImports::ResolveComponent,
  VueImports::ResolveDirective,
  VueImports::ResolveDynamicComponent,
  VueImports::Suspense,
  VueImports::Teleport,
  VueImports::ToDisplayString,
  VueImports::Transition,
  VueImports::TransitionGroup,
  VueImports::VModelCheckbox,
  VueImports::VModelRadio,
  VueImports::VModelSelect,
  VueImports::VModelText,
  VueImports::VShow,
  VueImports::WithCtx,
  VueImports::WithDirectives,
  VueImports::WithModifiers
];

impl <'a> CodegenContext <'a> {
  pub fn add_to_imports(self: &mut Self, vue_import: VueImports) {
    self.used_imports |= Self::get_import_mask_bit(vue_import);
  }

  pub fn get_import_str(vue_import: VueImports) -> &'static str {
    match vue_import {
      VueImports::CreateBlock => CREATE_BLOCK,
      VueImports::CreateCommentVNode => CREATE_COMMENT_VNODE,
      VueImports::CreateElementBlock => CREATE_ELEMENT_BLOCK,
      VueImports::CreateElementVNode => CREATE_ELEMENT_VNODE,
      VueImports::CreateTextVNode => CREATE_TEXT_VNODE,
      VueImports::CreateVNode => CREATE_VNODE,
      VueImports::Fragment => FRAGMENT,
      VueImports::KeepAlive => KEEP_ALIVE,
      VueImports::NormalizeClass => NORMALIZE_CLASS,
      VueImports::NormalizeStyle => NORMALIZE_STYLE,
      VueImports::OpenBlock => OPEN_BLOCK,
      VueImports::RenderList => RENDER_LIST,
      VueImports::RenderSlot => RENDER_SLOT,
      VueImports::ResolveComponent => RESOLVE_COMPONENT,
      VueImports::ResolveDirective => RESOLVE_DIRECTIVE,
      VueImports::ResolveDynamicComponent => RESOLVE_DYNAMIC_COMPONENT,
      VueImports::Suspense => SUSPENSE,
      VueImports::Teleport => TELEPORT,
      VueImports::ToDisplayString => TO_DISPLAY_STRING,
      VueImports::Transition => TRANSITION,
      VueImports::TransitionGroup => TRANSITION_GROUP,
      VueImports::VModelCheckbox => VMODEL_CHECKBOX,
      VueImports::VModelRadio => VMODEL_RADIO,
      VueImports::VModelSelect => VMODEL_SELECT,
      VueImports::VModelText => VMODEL_TEXT,
      VueImports::VShow => VSHOW,
      VueImports::WithCtx => WITH_CTX,
      VueImports::WithDirectives => WITH_DIRECTIVES,
      VueImports::WithModifiers => WITH_MODIFIERS,
    }
  }

  pub fn get_and_add_import_str(self: &mut Self, vue_import: VueImports) -> &'static str {
    self.add_to_imports(vue_import);

    Self::get_import_str(vue_import)
  }

  pub fn generate_imports_string(self: &Self) -> String {
    let imports_mask = self.used_imports;
    let mut result = String::from("import { ");
    let mut has_first_import = false;

    for import in ALL_IMPORTS.iter() {
      if Self::get_import_mask_bit(*import) & imports_mask == 0 {
        continue;
      }

      if has_first_import {
        result.push_str(", ");
      }

      let import_str = Self::get_import_str(*import);
      result.push_str(&import_str[1..]);
      result.push_str(" as ");
      result.push_str(import_str);

      has_first_import = true;
    }

    result.push_str(" } from \"vue\"");

    result
  }

  fn get_import_mask_bit(vue_import: VueImports) -> u64 {
    match vue_import {
      VueImports::CreateBlock =>              1<<0,
      VueImports::CreateCommentVNode =>       1<<1,
      VueImports::CreateElementBlock =>       1<<2,
      VueImports::CreateElementVNode =>       1<<3,
      VueImports::CreateTextVNode =>          1<<4,
      VueImports::CreateVNode =>              1<<5,
      VueImports::Fragment =>                 1<<6,
      VueImports::KeepAlive =>                1<<7,
      VueImports::NormalizeClass =>           1<<25,
      VueImports::NormalizeStyle =>           1<<26,
      VueImports::OpenBlock =>                1<<34,
      VueImports::RenderList =>               1<<35,
      VueImports::RenderSlot =>               1<<36,
      VueImports::ResolveComponent =>         1<<37,
      VueImports::ResolveDirective =>         1<<38,
      VueImports::ResolveDynamicComponent =>  1<<39,
      VueImports::Suspense =>                 1<<40,
      VueImports::Teleport =>                 1<<41,
      VueImports::ToDisplayString =>          1<<42,
      VueImports::Transition =>               1<<43,
      VueImports::TransitionGroup =>          1<<44,
      VueImports::VModelCheckbox =>           1<<45,
      VueImports::VModelRadio =>              1<<46,
      VueImports::VModelSelect =>             1<<47,
      VueImports::VModelText =>               1<<48,
      VueImports::VShow =>                    1<<49,
      VueImports::WithCtx =>                  1<<55,
      VueImports::WithDirectives =>           1<<56,
      VueImports::WithModifiers =>            1<<57,
    }
  }
}
