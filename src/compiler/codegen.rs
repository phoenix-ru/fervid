extern crate regex;

use std::collections::HashMap;
use regex::Regex;

use crate::parser::{Node, attributes::HtmlAttribute, StartingTag};
use super::{all_html_tags::is_html_tag, helper::CodeHelper};

const EXPORT_DEFAULT: &str = "export default ";
const CONST_SFC: &str = "const __sfc__ = ";
const CONST_SFC_EMPTY: &str = "const __sfc__ = {}"; // to optimize insertion

#[derive(Default)]
pub struct CodegenContext <'a> {
  pub code_helper: CodeHelper<'a>,
  pub components: HashMap<&'a str, String>,
  pub used_imports: u64,
  hoists: Vec<String>,
  is_custom_element: IsCustomElementParam<'a>
}

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

pub fn compile_sfc(blocks: &[Node]) -> Result<String, i32> {
  let mut result = String::new();
  let mut had_script = false;
  let mut had_template = false;

  // Todo optimize blocks processing order to not do any `insert`s

  for block in blocks.iter() {
    match block {
      Node::ElementNode { starting_tag, children } => {
        // Template is supported if it doesn't have a `lang` attr or has `lang="html"`
        let is_supported_template = starting_tag.tag_name == "template" &&
          !starting_tag.attributes.iter().any(|attr| match attr {
            HtmlAttribute::Regular { name, value } => {
              *name == "lang" && *value != "html"
            },
            _ => false
          });

        // Compile if we support it
        // todo check if we have double templates (do this in analyzer)
        if is_supported_template {
          let compiled_template = compile_template(&block)?;
          result.push_str(&compiled_template);
          had_template = true;
          continue;
        }

        // Script is supported if it has empty `lang`, `lang="js"` or `lang="ts"`
        let is_supported_script = starting_tag.tag_name == "script" &&
          children.len() > 0 &&
          !starting_tag.attributes.iter().any(|attr| match attr {
            HtmlAttribute::Regular { name, value } => {
              *name == "lang" && (*value != "js" || *value != "ts")
            },
            _ => false
          });

        // Naive approach to use script: just replace `export default ` with `const __sfc__ = `
        // Todo use real parser in the future (e.g. swc_ecma_parser)
        // Todo support at max 2 script elements (do so in analyzer)
        if is_supported_script {
          // We checked children earlier, so this should return the text of TextNode inside
          let script_content = children.get(0).map_or(
            Err(-2),
            |first_child| match first_child {
              Node::TextNode(v) => Ok(*v),
              _ => Err(-3)
            }
          )?;

          // Naive approach: replace
          // I don't know how to do this with an initial buffer without using `insert_str` three times
          let has_default_export = script_content.contains(EXPORT_DEFAULT);

          if has_default_export {
            result.insert_str(0, &script_content.replace(EXPORT_DEFAULT, CONST_SFC));
          }

          had_script = true;
          continue;
        }
      },

      _ => {
        // do what?
      }
    }
  }

  if result.is_empty() {
    return Err(-1000)
  }

  if !had_script {
    result.insert_str(0, CONST_SFC_EMPTY);
  }

  if had_template {
    result.push('\n');
    result.push_str("__sfc__.render = render");
  }

  result.push('\n');
  result.push_str("export default __sfc__");

  Ok(result)
}

/**
 * Main entry point for the code generation
 */
pub fn compile_template(template: &Node) -> Result<String, i32> {
  // Todo from options
  // Todo get context from the caller (compile_sfc)
  let is_custom_element_param = IsCustomElementParamRaw::Regex("custom-");
  let is_custom_element_re = match is_custom_element_param {
    IsCustomElementParamRaw::Regex(re) => IsCustomElementParam::Regex(Regex::new(re).expect("Invalid isCustomElement regex")),
    IsCustomElementParamRaw::String(string) => IsCustomElementParam::String(string),
    IsCustomElementParamRaw::None => IsCustomElementParam::None
  };

  /* Create the context */
  let mut ctx: CodegenContext = Default::default();
  ctx.is_custom_element = is_custom_element_re;

  // Try compiling the template. Indent because this will end up in a body of a function.
  // We first need to compile template before knowing imports, components and hoists
  ctx.code_helper.indent();
  let compiled_template = ctx.compile_node(&template);
  ctx.code_helper.unindent();

  if let Some(render_fn_return) = compiled_template {
    let mut result = ctx.generate_imports_string();
    ctx.code_helper.newline_n(&mut result, 2);

    // Function header
    result.push_str("function render(_ctx, _cache, $props, $setup, $data, $options) {");
    ctx.code_helper.indent();

    // Write components
    ctx.code_helper.newline(&mut result);
    ctx.generate_components_string(&mut result);

    // Write return statement
    ctx.code_helper.newline_n(&mut result, 2);
    result.push_str("return ");
    result.push_str(&render_fn_return);

    // Closing bracket
    ctx.code_helper.unindent();
    ctx.code_helper.newline(&mut result);
    result.push('}');

    Ok(result)
  } else {
    Err(-1)
  }

  // // Debug info: show used imports
  // println!("Used imports: {}", ctx.generate_imports_string());
  // println!();

  // // Debug info: show used imports
  // println!("Used components: {:?}", ctx.components);
  // println!();

  // // Zero really means nothing. It's just that error handling is not yet implemented
  // test_res.ok_or(0)
}

impl <'a> CodegenContext <'a> {
  pub fn compile_node(self: &mut Self, node: &'a Node) -> Option<String> {
    match node {
      Node::ElementNode { starting_tag, children } => {
        let mut buf = String::new();
        self.create_element_vnode(&mut buf, starting_tag, children);
        Some(buf)
      },
      _ => None
    }
  }

  pub fn compile_node_children(self: &mut Self, children: &Vec<Node>) {
    todo!()
  }

  fn add_to_hoists(self: &mut Self, expression: String) -> String {
    let hoist_index = self.hoists.len() + 1;
    let hoist_identifier = format!("_hoisted_{}", hoist_index);

    // todo add pure in consumer instead or provide a boolean flag to generate it
    let hoist_expr = format!("const {} = /*#__PURE__*/ {}", hoist_identifier, expression);
    self.hoists.push(hoist_expr);

    hoist_identifier
  }

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

  /**
   * The element can be hoisted if it and all of its descendants do not have dynamic attributes
   * <div class="a"><span>text</span></div> => true
   * <button :disabled="isDisabled">text</button> => false
   * <span>{{ text }}</span> => false
   */
  fn can_be_hoisted (self: &Self, node: &Node) -> bool {
    match node {
      Node::ElementNode { starting_tag, children } => { 
        /* Check starting tag */
        if self.is_component(starting_tag) {
          return false;
        }

        let has_any_dynamic_attr = starting_tag.attributes.iter().any(|it| {
          match it {
            HtmlAttribute::Regular { .. } => false,
            HtmlAttribute::VDirective { .. } => true
          }
        });

        if has_any_dynamic_attr {
          return false;
        }

        let cannot_be_hoisted = children.iter().any(|it| !self.can_be_hoisted(&it));

        return !cannot_be_hoisted;
      },

      Node::TextNode(_) => {
        return true
      },

      Node::DynamicExpression(_) => {
        return false
      }
    };
  }
}
