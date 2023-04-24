/// A Node represents a part of the Abstract Syntax Tree (AST).
/// There are several possible Node types:
///
/// ### `ElementNode`
/// It means that the node is a basic HTML tag node.
///
/// `ElementNode` has a starting `<tag>` with attributes,
///   zero or more children and a closing `</tag>` unless this node is self-closed `<tag />`.
///   The parser does not add any meaning to the discovered tag name,
///   as this logic is application-specific.
///
/// ### `TextNode`
/// These nodes are the basic HTML text leaf nodes
///   which can only contain static text.
///
/// ### `DynamicExpression`
/// Dynamic expression is a special syntax for Vue templates.
///
/// It looks like this: `{{ some + js - expression }}`,
/// where the content inside `{{` and `}}` delimiters is arbitrary.
///
/// ### `CommentNode`
/// `CommentNode` is the vanilla HTML comment, which looks like this: `<-- this is comment -->`
#[derive(Debug, Clone)]
pub enum Node<'a> {
  ElementNode(ElementNode<'a>),
  TextNode(&'a str),
  DynamicExpression { value: &'a str, template_scope: u32 },
  CommentNode(&'a str)
}

/// Element node is a classic HTML node with some added functionality:
/// 1. Its starting tag can have Vue directives as attributes;
/// 2. It may have [`Node::DynamicExpression`] as a child;
/// 3. It has a `template_scope` assigned, which is responsible
///    for the correct compilation of dynamic bindings and expressions.
#[derive(Debug, Clone)]
pub struct ElementNode<'a> {
  pub starting_tag: StartingTag<'a>,
  pub children: Vec<Node<'a>>,
  pub template_scope: u32
}

/// Starting tag represents [`ElementNode`]'s tag name and attributes
#[derive(Debug, Clone)]
pub struct StartingTag<'a> {
  pub tag_name: &'a str,
  pub attributes: Vec<HtmlAttribute<'a>>,
  pub is_self_closing: bool,
  pub kind: ElementKind
}

/// Attribute may either be `Regular` (static) or a `VDirective` (application-specific)
#[derive(Debug, Clone)]
pub enum HtmlAttribute <'a> {
  Regular {
    name: &'a str,
    value: &'a str
  },
  VDirective(VDirective<'a>)
}

#[derive(Debug, Default, Clone)]
pub struct VDirective<'a> {
  pub name: &'a str,
  pub argument: &'a str,
  pub modifiers: Vec<&'a str>,
  pub value: Option<&'a str>,
  pub is_dynamic_slot: bool
}

#[derive(Debug, Clone)]
pub enum ElementKind {
  Void,
  RawText,
  RCData,
  Foreign,
  Normal
}

pub struct SfcTemplateBlock<'a> {
  pub lang: &'a str,
  pub roots: &'a [Node<'a>],
}

pub struct SfcScriptBlock<'a> {
  pub lang: &'a str,
  pub content: &'a str,
  pub is_setup: bool,
}

pub struct SfcStyleBlock<'a> {
  pub lang: &'a str,
  pub content: &'a str,
  pub is_scoped: bool,
}

pub enum SfcBlock<'a> {
  Template(SfcTemplateBlock<'a>),
  Script(SfcScriptBlock<'a>),
  Style(SfcStyleBlock<'a>),
  Custom(&'a ElementNode<'a>),
}
