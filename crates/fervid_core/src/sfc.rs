use swc_core::{common::Span, ecma::ast::Module};

use crate::{FervidAtom, Node, StartingTag};

#[derive(Debug, Default)]
pub struct SfcDescriptor {
    pub template: Option<SfcTemplateBlock>,
    pub script_legacy: Option<SfcScriptBlock>,
    pub script_setup: Option<SfcScriptBlock>,
    pub styles: Vec<SfcStyleBlock>,
    pub custom_blocks: Vec<SfcCustomBlock>,
}

#[derive(Clone, Debug)]
pub struct SfcTemplateBlock {
    pub lang: FervidAtom,
    pub roots: Vec<Node>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SfcScriptBlock {
    pub content: Box<Module>,
    pub lang: SfcScriptLang,
    pub is_setup: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SfcStyleBlock {
    pub lang: FervidAtom,
    pub content: FervidAtom,
    pub is_scoped: bool,
    pub is_module: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SfcCustomBlock {
    pub starting_tag: StartingTag,
    pub content: FervidAtom,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum SfcScriptLang {
    Es,
    Typescript,
}
