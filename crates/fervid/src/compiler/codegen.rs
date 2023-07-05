extern crate regex;

use fxhash::FxHashMap as HashMap;
use fervid_core::{SfcTemplateBlock, Node, SfcScriptBlock, SfcStyleBlock, SfcBlock, StartingTag};
use regex::Regex;

use crate::analyzer::scope::ScopeHelper;
use super::{all_html_tags::is_html_tag, helper::CodeHelper};

#[derive(Default)]
pub struct CodegenContext <'a> {
  pub code_helper: CodeHelper<'a>,
  pub components: HashMap<String, String>,
  pub directives: HashMap<String, String>,
  pub used_imports: u64,
  pub scope_helper: ScopeHelper,
  // hoists: Vec<String>,
  is_custom_element: IsCustomElementParam<'a>
}

#[allow(dead_code)]
enum IsCustomElementParamRaw <'a> {
  String(&'a str),
  Regex(&'a str),
  None
}

enum IsCustomElementParam <'a> {
  String(&'a str),
  Regex(Regex),
  None
}

impl Default for IsCustomElementParam<'_> {
  fn default() -> Self {
    Self::None
  }
}

/**
 * Main entry point for the code generation
 * TODO REFACTOR
 */
pub fn compile_sfc(blocks: Vec<SfcBlock>, scope_helper: ScopeHelper) -> Result<String, i32> {
  let mut template: Option<SfcTemplateBlock> = None;
  let mut legacy_script: Option<SfcScriptBlock> = None;
  let mut setup_script: Option<SfcScriptBlock> = None;
  #[allow(unused)]
  let mut style: Option<SfcStyleBlock> = None;

  for block in blocks.into_iter() {
    match block {
      SfcBlock::Template(template_block) => template = Some(template_block),
      SfcBlock::Script(script_block) => {
        if script_block.is_setup {
          setup_script = Some(script_block);
        } else {
          legacy_script = Some(script_block);
        }
      },
      #[allow(unused_assignments)]
      SfcBlock::Style(style_block) => style = Some(style_block),
      _ => {}
    }
  }

  // Check that there is some work to do
  if !(template.is_some() || legacy_script.is_some() || setup_script.is_some()) {
    return Err(-1000); // todo error enums or anyhow
  }

  // Resulting buffer
  let mut result = String::new();

  // Todo from options
  let is_custom_element_param = IsCustomElementParamRaw::Regex("custom-");
  let is_custom_element_re = match is_custom_element_param {
    IsCustomElementParamRaw::Regex(re) => IsCustomElementParam::Regex(Regex::new(re).expect("Invalid isCustomElement regex")),
    IsCustomElementParamRaw::String(string) => IsCustomElementParam::String(string),
    IsCustomElementParamRaw::None => IsCustomElementParam::None
  };

  /* Create the context */
  let mut ctx: CodegenContext = Default::default();
  ctx.is_custom_element = is_custom_element_re;
  ctx.scope_helper = scope_helper;

  // Todo generate imports and hoists first in PROD mode (but this requires a smarter order of compilation)

  // Generate scripts
  ctx.compile_scripts(&mut result, legacy_script, setup_script);

  // Generate template
  if let Some(template_node) = template {
    // todo do not allocate extra strings in functions?
    // yet, we still need to hold the results of template render fn to append it later

    // Get newline size hint for later
    let newline_byte_size = ctx.code_helper.newline_size_hint();

    // First, compile the template to its own String. This will also determine all the imports
    let compiled_template = ctx.compile_template(template_node)?;

    // Second, generate the imports String...
    let imports = ctx.generate_imports_string();
    // ... extend the capacity with a generous size hint...
    result.reserve(imports.len() + compiled_template.len() + 4 * newline_byte_size + 23 + 22);
    // ... and append to SFC codegen result
    result.push_str(&imports);
    ctx.code_helper.newline_n(&mut result, 2);

    // Third, push the compiled template
    result.push_str(&compiled_template);

    // And finally, assign the render fn to __sfc__
    // todo this is only for DEV
    ctx.code_helper.newline(&mut result);
    result.push_str("__sfc__.render = render");
  }

  ctx.code_helper.newline(&mut result);
  result.push_str("export default __sfc__");

  Ok(result)
}

impl <'a> CodegenContext <'a> {
  pub fn compile_template(&mut self, template: SfcTemplateBlock) -> Result<String, i32> {
    // Try compiling the template. Indent because this will end up in a body of a function.
    // We first need to compile template before knowing imports, components and hoists
    self.code_helper.indent();
    let mut compiled_template = String::new();

    // todo better handling of multiple root children (use Fragment)
    self.compile_node(&mut compiled_template, &template.roots[0], true);

    self.code_helper.unindent();

    // todo do not generate this inside compile_template, as PROD mode puts it to the top
    let mut result = String::new();

    // Function header
    result.push_str("function render(_ctx, _cache, $props, $setup, $data, $options) {");
    self.code_helper.indent();

    // Write components
    if self.components.len() > 0 {
      self.code_helper.newline(&mut result);
      self.generate_components_string(&mut result);
    }

    // Write directives
    if self.directives.len() > 0 {
      self.code_helper.newline(&mut result);
      self.generate_directive_resolves(&mut result);
    }

    // Write return statement
    self.code_helper.newline_n(&mut result, 2);
    result.push_str("return ");
    result.push_str(&compiled_template);

    // Closing bracket
    self.code_helper.unindent();
    self.code_helper.newline(&mut result);
    result.push('}');

    Ok(result)
  }

  pub fn compile_node(&mut self, buf: &mut String, node: &Node, wrap_in_block: bool) {
    // todo add the code for `openBlock`, `createElementBlock` and Fragments when needed
    match node {
      Node::Element(element_node) => {
        if self.is_component(&element_node.starting_tag) {
          self.create_component_vnode(buf, element_node, wrap_in_block);
        } else if let Some(builtin_type) = Self::is_builtin(element_node) {
          self.compile_builtin(buf, element_node, builtin_type);
        } else {
          self.create_element_vnode(buf, element_node, wrap_in_block);
        }
      },

      Node::Comment(comment) => {
        self.create_comment_vnode(buf, &comment)
      },

      _ => {}
    }
  }

  // fn add_to_hoists(self: &mut Self, expression: String) -> String {
  //   let hoist_index = self.hoists.len() + 1;
  //   let hoist_identifier = format!("_hoisted_{}", hoist_index);

  //   // todo add pure in consumer instead or provide a boolean flag to generate it
  //   let hoist_expr = format!("const {} = /*#__PURE__*/ {}", hoist_identifier, expression);
  //   self.hoists.push(hoist_expr);

  //   hoist_identifier
  // }

  pub fn is_component(self: &Self, starting_tag: &StartingTag) -> bool {
    // todo use analyzed components? (fields of `components: {}`)

    let tag_name = starting_tag.tag_name;

    let is_html_tag = is_html_tag(tag_name);
    if is_html_tag {
      return false;
    }

    /* Check with isCustomElement */
    let is_custom_element = match &self.is_custom_element {
      IsCustomElementParam::String(string) => tag_name == *string,
      IsCustomElementParam::Regex(re) => re.is_match(tag_name),
      IsCustomElementParam::None => false
    };

    !is_custom_element
  }

  /*
   * The element can be hoisted if it and all of its descendants do not have dynamic attributes
   * <div class="a"><span>text</span></div> => true
   * <button :disabled="isDisabled">text</button> => false
   * <span>{{ text }}</span> => false
   */
  // fn can_be_hoisted (self: &Self, node: &Node) -> bool {
  //   match node {
  //     Node::ElementNode(ElementNode { starting_tag, children, .. }) => { 
  //       /* Check starting tag */
  //       if self.is_component(starting_tag) {
  //         return false;
  //       }

  //       let has_any_dynamic_attr = starting_tag.attributes.iter().any(|it| {
  //         match it {
  //           HtmlAttribute::Regular { .. } => false,
  //           HtmlAttribute::VDirective { .. } => true
  //         }
  //       });

  //       if has_any_dynamic_attr {
  //         return false;
  //       }

  //       let cannot_be_hoisted = children.iter().any(|it| !self.can_be_hoisted(&it));

  //       return !cannot_be_hoisted;
  //     },

  //     Node::TextNode(_) => {
  //       return true
  //     },

  //     Node::DynamicExpression { .. } => {
  //       return false
  //     },

  //     Node::CommentNode(_) => {
  //       return false;
  //     }
  //   };
  // }
}
