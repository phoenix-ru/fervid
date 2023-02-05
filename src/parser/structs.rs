use super::{attributes::HtmlAttribute, html_utils::ElementKind};

#[derive(Debug, Clone)]
pub enum Node<'a> {
  ElementNode(ElementNode<'a>),

  TextNode(&'a str),
  DynamicExpression(&'a str),
  CommentNode(&'a str)
}

#[derive(Debug, Clone)]
pub struct ElementNode<'a> {
  pub starting_tag: StartingTag<'a>,
  pub children: Vec<Node<'a>>
}

#[derive(Debug, Clone)]
pub struct StartingTag<'a> {
  pub tag_name: &'a str,
  pub attributes: Vec<HtmlAttribute<'a>>,
  pub is_self_closing: bool,
  pub kind: ElementKind
}
