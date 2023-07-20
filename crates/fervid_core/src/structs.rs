use swc_core::ecma::ast::{Expr, Pat};

/// A Node represents a part of the Abstract Syntax Tree (AST).
#[derive(Debug, Clone)]
pub enum Node<'a> {
    /// `Element` means that the node is a basic HTML tag node.
    ///
    /// `Element` has a starting `<tag>` with attributes,
    ///   zero or more children and a closing `</tag>` unless this node is self-closed `<tag />`.
    ///   The parser does not add any meaning to the discovered tag name,
    ///   as this logic is application-specific.
    Element(ElementNode<'a>),

    /// These nodes are the basic HTML text leaf nodes
    /// which can only contain static text.
    Text(&'a str),

    /// Interpolation is a special syntax for Vue templates.
    ///
    /// It looks like this: `{{ some + js - expression }}`,
    /// where the content inside `{{` and `}}` delimiters is arbitrary.
    Interpolation(Interpolation),

    /// `Comment` is the vanilla HTML comment, which looks like this: `<-- this is comment -->`
    Comment(&'a str),

    /// `ConditionalSeq` is a representation of `v-if`/`v-else-if`/`v-else` node sequence.
    /// Its children are the other `Node`s, this node is just a wrapper.
    ConditionalSeq(ConditionalNodeSequence<'a>),
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
    pub template_scope: u32,
}

/// This is a synthetic node type only available after AST optimizations.
/// Its purpose is to make conditional code generation trivial.\
/// The `ConditionalNodeSequence` consists of:
/// - exactly one `v-if` `ElementNode`;
/// - 0 or more `v-else-if` `ElementNode`s;
/// - 0 or 1 `v-else` `ElementNode`.
#[derive(Debug, Clone)]
pub struct ConditionalNodeSequence<'a> {
    pub if_node: Box<Conditional<'a>>,
    pub else_if_nodes: Vec<Conditional<'a>>,
    pub else_node: Option<Box<ElementNode<'a>>>,
}

#[derive(Debug, Clone)]
pub struct Conditional<'e> {
    pub condition: Expr,
    pub node: ElementNode<'e>
}

#[derive(Debug, Clone)]
pub struct Interpolation {
    pub value: Box<Expr>,
    pub template_scope: u32,
    pub patch_flag: bool
}

/// Starting tag represents [`ElementNode`]'s tag name and attributes
#[derive(Debug, Clone)]
pub struct StartingTag<'a> {
    pub tag_name: &'a str,
    pub attributes: Vec<AttributeOrBinding<'a>>,
    pub directives: Option<Box<VueDirectives<'a>>>,
}

/// Denotes the basic attributes or bindings of a DOM element
/// As of directives, this only covers `v-bind` and `v-on`,
/// because they bind something to DOM.
/// `v-model` is not covered here because its code generation is not as trivial.
#[derive(Debug, Clone)]
pub enum AttributeOrBinding<'a> {
    /// `RegularAttribute` is a plain HTML attribute without any associated logic
    RegularAttribute { name: &'a str, value: &'a str },
    /// `v-bind` directive
    VBind(VBindDirective<'a>),
    /// `v-on` directive
    VOn(VOnDirective<'a>),
}

/// Describes a type which can be either a static &str or a js Expr.
/// This is mostly usable for dynamic binding scenarios.
/// ## Example
/// - `:foo="bar"` yields `StrOrExpr::Str("foo")`;
/// - `:[baz]="qux"` yields `StrOrExpr::Expr(Box::new(Expr::Lit(Lit::Str(Str { value: "baz".into(), .. }))))`
#[derive(Debug, Clone)]
pub enum StrOrExpr<'s> {
    Str(&'s str),
    Expr(Box<Expr>)
}

impl <'s> From<&'s str> for StrOrExpr<'s> {
    fn from(value: &'s str) -> StrOrExpr<'s> {
        StrOrExpr::Str(value)
    }
}

#[derive(Clone, Debug, Default)]
pub struct VueDirectives<'d> {
    pub custom: Vec<VCustomDirective<'d>>,
    pub v_cloak: Option<()>,
    pub v_else: Option<()>,
    pub v_else_if: Option<Box<Expr>>,
    pub v_for: Option<VForDirective>,
    pub v_html: Option<Box<Expr>>,
    pub v_if: Option<Box<Expr>>,
    pub v_memo: Option<Box<Expr>>,
    pub v_model: Vec<VModelDirective<'d>>,
    pub v_once: Option<()>,
    pub v_pre: Option<()>,
    pub v_show: Option<Box<Expr>>,
    pub v_slot: Option<VSlotDirective<'d>>,
    pub v_text: Option<Box<Expr>>,
}

#[derive(Clone, Debug)]
pub struct VForDirective {
    /// `bar` in `v-for="foo in bar"`
    pub iterable: Box<Expr>,
    /// `foo` in `v-for="foo in bar"`
    pub itervar: Box<Expr>,
}

#[derive(Clone, Debug)]
pub struct VOnDirective<'a> {
    /// What event to listen to. If None, it is equivalent to `v-on="..."`.
    pub event: Option<StrOrExpr<'a>>,
    /// What is the handler to use. If None, `modifiers` must not be empty.
    pub handler: Option<Box<Expr>>,
    /// A list of modifiers after the dot, e.g. `stop` and `prevent` in `@click.stop.prevent="handleClick"`
    pub modifiers: Vec<&'a str>,
}

#[derive(Clone, Debug)]
pub struct VBindDirective<'a> {
    /// Attribute name to bind. If None, it is equivalent to `v-bind="..."`.
    pub argument: Option<StrOrExpr<'a>>,
    /// Attribute value, e.g. `smth` in `:attr="smth"`
    pub value: Box<Expr>,
    /// .camel modifier
    pub is_camel: bool,
    /// .prop modifier
    pub is_prop: bool,
    /// .attr modifier
    pub is_attr: bool,
}

#[derive(Clone, Debug)]
pub struct VModelDirective<'a> {
    /// What to apply v-model to, e.g. `first-name` in `v-model:first-name="first"`
    pub argument: Option<&'a str>,
    /// The binding of a `v-model`, e.g. `userInput` in `v-model="userInput"`
    pub value: Expr,
    /// `lazy` and `trim` in `v-model.lazy.trim`
    pub modifiers: Vec<&'a str>,
}

#[derive(Clone, Debug)]
pub struct VSlotDirective<'a> {
    pub slot_name: Option<&'a str>,
    /// What bindings are provided to slot children, e.g. `value` in `v-slot="{ value }"`
    pub value: Option<Box<Pat>>,
    pub is_dynamic_slot: bool,
}

#[derive(Debug, Default, Clone)]
pub struct VCustomDirective<'a> {
    /// `foo` in `v-foo`
    pub name: &'a str,
    /// `bar` in `v-foo:bar`
    pub argument: Option<&'a str>,
    /// `baz` and `qux` in `v-foo:bar.baz.qux`
    pub modifiers: Vec<&'a str>,
    /// `loremIpsum` in `v-foo="loremIpsum"`
    pub value: Option<Box<Expr>>,
}

/// https://github.com/vuejs/core/blob/020851e57d9a9f727c6ea07e9c1575430af02b73/packages/compiler-core/src/options.ts#L76
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BindingTypes {
    /// returned from data()
    Data,
    /// declared as a prop
    Props,
    /// a local alias of a `<script setup>` destructured prop.
    /// the original is stored in __propsAliases of the bindingMetadata object.
    PropsAliased,
    /// a let binding (may or may not be a ref)
    SetupLet,
    /// a const binding that can never be a ref.
    /// these bindings don't need `unref()` calls when processed in inlined
    /// template expressions.
    SetupConst,
    /// a const binding that does not need `unref()`, but may be mutated.
    SetupReactiveConst,
    /// a const binding that may be a ref
    SetupMaybeRef,
    /// bindings that are guaranteed to be refs
    SetupRef,
    /// declared by other options, e.g. computed, inject
    Options,
    /// a literal constant, e.g. 'foo', 1, true
    LiteralConst,
    /// a variable from the template
    TemplateLocal,
    /// a variable in the global Javascript context, e.g. `Array` or `undefined`
    JsGlobal,
    /// a non-resolved variable, presumably from the global Vue context
    Unresolved,
}
