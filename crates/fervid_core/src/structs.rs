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
  pub attributes: Vec<HtmlAttribute<'a>>
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

#[derive(Clone, Debug)]
pub enum VDirective<'a> {
  Bind(VBindDirective<'a>),
  Cloak,
  Custom(VCustomDirective<'a>),
  Else,
  ElseIf(&'a str),
  For(VForDirective<'a>),
  Html(&'a str),
  If(&'a str),
  Memo(&'a str),
  Model(VModelDirective<'a>),
  On(VOnDirective<'a>),
  Once,
  Pre,
  Show(&'a str),
  Slot(VSlotDirective<'a>),
  Text(&'a str),
}

#[derive(Clone, Debug)]
pub struct VForDirective<'a> {
  pub iterable: &'a str,
  pub iterator: &'a str
}

#[derive(Clone, Debug)]
pub struct VOnDirective<'a> {
  /// What event to listen to. If None, that is equal to `v-on="smth"`. Also, see `is_dynamic_event`.
  pub event: Option<&'a str>,
  /// What is the handler to use. If None, `modifiers` must not be empty.
  pub handler: Option<&'a str>,
  /// If the event itself is dynamic, e.g. `v-on:[event]` or `@[event]`
  pub is_dynamic_event: bool,
  /// A list of modifiers after the dot, e.g. `stop` and `prevent` in `@click.stop.prevent="handleClick"`
  pub modifiers: Vec<&'a str>
}

#[derive(Clone, Debug, Default)]
pub struct VBindDirective<'a> {
  /// Attribute name to bind. If None, it is equivalent to `v-bind="smth"`. Also, see `is_dynamic_attr`.
  pub argument: Option<&'a str>,
  /// Attribute value, e.g. `smth` in `:attr="smth"`
  pub value: &'a str,
  /// `:[dynamic]`
  pub is_dynamic_attr: bool,
  /// .camel modifier
  pub is_camel: bool,
  /// .prop modifier
  pub is_prop: bool,
  /// .attr modifier
  pub is_attr: bool
}

#[derive(Clone, Debug)]
pub struct VModelDirective<'a> {
  /// What to apply v-model to, e.g. `first-name` in `v-model:first-name="first"`
  pub argument: Option<&'a str>,
  pub value: &'a str,
  pub modifiers: Vec<&'a str>
}

#[derive(Clone, Debug)]
pub struct VSlotDirective<'a> {
  pub slot_name: Option<&'a str>,
  pub value: Option<&'a str>,
  pub is_dynamic_slot: bool
}

#[derive(Debug, Default, Clone)]
pub struct VCustomDirective<'a> {
  pub name: &'a str,
  pub argument: Option<&'a str>,
  pub modifiers: Vec<&'a str>,
  pub value: Option<&'a str>
}
