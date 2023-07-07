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

    /// Dynamic expression is a special syntax for Vue templates.
    ///
    /// It looks like this: `{{ some + js - expression }}`,
    /// where the content inside `{{` and `}}` delimiters is arbitrary.
    DynamicExpression { value: &'a str, template_scope: u32 },

    /// `Comment` is the vanilla HTML comment, which looks like this: `<-- this is comment -->`
    Comment(&'a str),

    /// `ConditionalSeq` is a representation of `v-if`/`v-else-if`/`v-else` node sequence.
    /// Its children are the other `Node`s, this node is just a wrapper.
    ConditionalSeq(ConditionalNodeSequence<'a>)
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
    pub if_node: (&'a str, Box<ElementNode<'a>>),
    pub else_if_nodes: Vec<(&'a str, ElementNode<'a>)>,
    pub else_node: Option<Box<ElementNode<'a>>>
}

/// Starting tag represents [`ElementNode`]'s tag name and attributes
#[derive(Debug, Clone)]
pub struct StartingTag<'a> {
    pub tag_name: &'a str,
    pub attributes: Vec<AttributeOrBinding<'a>>,
    pub directives: Option<Box<VueDirectives<'a>>>
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

#[derive(Clone, Debug, Default)]
pub struct VueDirectives<'d> {
    pub custom: Vec<VCustomDirective<'d>>,
    pub v_cloak: Option<()>,
    pub v_else: Option<()>,
    pub v_else_if: Option<&'d str>,
    pub v_for: Option<VForDirective<'d>>,
    pub v_html: Option<&'d str>,
    pub v_if: Option<&'d str>,
    pub v_memo: Option<&'d str>,
    pub v_model: Vec<VModelDirective<'d>>,
    pub v_once: Option<()>,
    pub v_pre: Option<()>,
    pub v_show: Option<&'d str>,
    pub v_slot: Option<VSlotDirective<'d>>,
    pub v_text: Option<&'d str>
}

#[derive(Clone, Debug)]
pub struct VForDirective<'a> {
    pub iterable: &'a str,
    pub iterator: &'a str,
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
    pub modifiers: Vec<&'a str>,
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
    pub is_attr: bool,
}

#[derive(Clone, Debug)]
pub struct VModelDirective<'a> {
    /// What to apply v-model to, e.g. `first-name` in `v-model:first-name="first"`
    pub argument: Option<&'a str>,
    pub value: &'a str,
    pub modifiers: Vec<&'a str>,
}

#[derive(Clone, Debug)]
pub struct VSlotDirective<'a> {
    pub slot_name: Option<&'a str>,
    pub value: Option<&'a str>,
    pub is_dynamic_slot: bool,
}

#[derive(Debug, Default, Clone)]
pub struct VCustomDirective<'a> {
    pub name: &'a str,
    pub argument: Option<&'a str>,
    pub modifiers: Vec<&'a str>,
    pub value: Option<&'a str>,
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
    Unresolved
}
