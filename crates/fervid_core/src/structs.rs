use swc_core::{ecma::{ast::{Expr, Pat}, atoms::JsWord}, common::Span};

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

    // /// `ForFragment` is a representation of a `v-for` node.
    // /// This type is for ergonomics,
    // /// i.e. to separate patch flags and `key` of the repeater from the repeatable.
    // ForFragment(ForFragment<'a>)
}

/// Element node is a classic HTML node with some added functionality:
/// 1. Its starting tag can have Vue directives as attributes;
/// 2. It may have [`Node::Interpolation`] as a child;
/// 3. It has a `template_scope` assigned, which is responsible
///    for the correct compilation of dynamic bindings and expressions.
#[derive(Debug, Clone)]
pub struct ElementNode<'a> {
    /// Marks the node as either an Element (HTML tag), Builtin (Vue) or Component
    pub kind: ElementKind,
    pub starting_tag: StartingTag<'a>,
    pub children: Vec<Node<'a>>,
    pub template_scope: u32,
    pub patch_hints: PatchHints,
    pub span: Span
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ElementKind {
    Builtin(BuiltinType),
    #[default]
    Element,
    Component,
}

#[derive(Debug, Clone, Copy)]
pub enum BuiltinType {
    Component,
    KeepAlive,
    Slot,
    Suspense,
    Teleport,
    Transition,
    TransitionGroup,
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
    pub node: ElementNode<'e>,
}

#[derive(Debug, Clone)]
pub struct Interpolation {
    pub value: Box<Expr>,
    pub template_scope: u32,
    pub patch_flag: bool,
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
    Expr(Box<Expr>),
}

impl<'s> From<&'s str> for StrOrExpr<'s> {
    fn from(value: &'s str) -> StrOrExpr<'s> {
        StrOrExpr::Str(value)
    }
}

#[derive(Debug, Default, Clone)]
pub struct PatchHints {
    /// Patch flags
    pub flags: PatchFlagsSet,
    /// Dynamic props
    pub props: Vec<JsWord>
}

flagset::flags! {
    /// From https://github.com/vuejs/core/blob/b8fc18c0b23be9a77b05dc41ed452a87a0becf82/packages/shared/src/patchFlags.ts
    #[derive(Default)]
    pub enum PatchFlags: i32 {
        /**
         * Indicates an element with dynamic textContent (children fast path)
         */
        Text = 1,

        /**
         * Indicates an element with dynamic class binding.
         */
        Class = 1 << 1,

        /**
         * Indicates an element with dynamic style
         * The compiler pre-compiles static string styles into static objects
         * + detects and hoists inline static objects
         * e.g. `style="color: red"` and `:style="{ color: 'red' }"` both get hoisted
         * as:
         * ```js
         * const style = { color: 'red' }
         * render() { return e('div', { style }) }
         * ```
         */
        Style = 1 << 2,

        /**
         * Indicates an element that has non-class/style dynamic props.
         * Can also be on a component that has any dynamic props (includes
         * class/style). when this flag is present, the vnode also has a dynamicProps
         * array that contains the keys of the props that may change so the runtime
         * can diff them faster (without having to worry about removed props)
         */
        Props = 1 << 3,

        /**
         * Indicates an element with props with dynamic keys. When keys change, a full
         * diff is always needed to remove the old key. This flag is mutually
         * exclusive with CLASS, STYLE and PROPS.
         */
        FullProps = 1 << 4,

        /**
         * Indicates an element with event listeners (which need to be attached
         * during hydration)
         */
        HydrateEvents = 1 << 5,

        /**
         * Indicates a fragment whose children order doesn't change.
         */
        StableFragment = 1 << 6,

        /**
         * Indicates a fragment with keyed or partially keyed children
         */
        KeyedFragment = 1 << 7,

        /**
         * Indicates a fragment with unkeyed children.
         */
        UnkeyedFragment = 1 << 8,

        /**
         * Indicates an element that only needs non-props patching, e.g. ref or
         * directives (onVnodeXXX hooks). since every patched vnode checks for refs
         * and onVnodeXXX hooks, it simply marks the vnode so that a parent block
         * will track it.
         */
        #[default]
        NeedPatch = 1 << 9,

        /**
         * Indicates a component with dynamic slots (e.g. slot that references a v-for
         * iterated value, or dynamic slot names).
         * Components with this flag are always force updated.
         */
        DynamicSlots = 1 << 10,

        /**
         * Indicates a fragment that was created only because the user has placed
         * comments at the root level of a template. This is a dev-only flag since
         * comments are stripped in production.
         */
        DevRootFragment = 1 << 11,

        /**
         * SPECIAL FLAGS -------------------------------------------------------------
         * Special flags are negative integers. They are never matched against using
         * bitwise operators (bitwise matching should only happen in branches where
         * patchFlag > 0), and are mutually exclusive. When checking for a special
         * flag, simply check patchFlag === FLAG.
         */

        /**
         * Indicates a hoisted static vnode. This is a hint for hydration to skip
         * the entire sub tree since static content never needs to be updated.
         */
        Hoisted = -1,
        /**
         * A special flag that indicates that the diffing algorithm should bail out
         * of optimized mode. For example, on block fragments created by renderSlot()
         * when encountering non-compiler generated slots (i.e. manually written
         * render functions, which should always be fully diffed)
         * OR manually cloneVNodes
         */
        Bail = -2,
    }
}

pub type PatchFlagsSet = flagset::FlagSet<PatchFlags>;

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
    pub span: Span
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

/// <https://github.com/vuejs/core/blob/020851e57d9a9f727c6ea07e9c1575430af02b73/packages/compiler-core/src/options.ts#L76>
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
