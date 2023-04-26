use crate::{Node, StartingTag};

pub struct SfcDescriptor<'a> {
  pub template: Option<SfcTemplateBlock<'a>>,
  pub script_legacy: Option<SfcScriptBlock<'a>>,
  pub script_setup: Option<SfcScriptBlock<'a>>,
  pub styles: Vec<SfcStyleBlock<'a>>,
  pub custom_blocks: Vec<SfcCustomBlock<'a>>
}

#[derive(Clone)]
pub struct SfcTemplateBlock<'a> {
  pub lang: &'a str,
  pub roots: Vec<Node<'a>>,
}

#[derive(Clone)]
pub struct SfcScriptBlock<'a> {
  pub lang: &'a str,
  pub content: &'a str,
  pub is_setup: bool,
}

#[derive(Clone)]
pub struct SfcStyleBlock<'a> {
  pub lang: &'a str,
  pub content: &'a str,
  pub is_scoped: bool,
}

#[derive(Clone)]
pub struct SfcCustomBlock<'a> {
  pub starting_tag: StartingTag<'a>,
  pub content: &'a str
}

#[derive(Clone)]
pub enum SfcBlock<'a> {
  Template(SfcTemplateBlock<'a>),
  Script(SfcScriptBlock<'a>),
  Style(SfcStyleBlock<'a>),
  Custom(SfcCustomBlock<'a>),
}
