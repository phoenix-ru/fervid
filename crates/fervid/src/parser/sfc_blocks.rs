use fervid_core::{ElementNode, Node, HtmlAttribute, SfcBlock, SfcTemplateBlock, SfcStyleBlock, SfcScriptBlock};

/// Converts an untyped root block (which is always a [`Node::ElementNode`]) to an [`SfcBlock`],
/// which is a Vue SFC descriptor block
pub fn convert_node_to_typed<'e>(node: &'e ElementNode) -> SfcBlock<'e> {
    let starting_tag = &node.starting_tag;

    // Mutually exclusive flags
    let is_script = starting_tag.tag_name == "script";
    let is_template = !is_script && starting_tag.tag_name == "template";
    let is_style = !is_template && starting_tag.tag_name == "style";

    if !is_script && !is_template && !is_style {
        return SfcBlock::Custom(&node);
    }

    // Get `lang` attribute, which is common for all the Vue root blocks
    let lang = starting_tag.attributes.iter().find_map(|attr| match attr {
        HtmlAttribute::Regular {
            name: "lang",
            value,
        } => Some(*value),
        _ => None,
    });

    // First, check for template, this is already parsed
    if is_template {
        return SfcBlock::Template(SfcTemplateBlock {
            lang: lang.unwrap_or("html"),
            roots: &node.children,
        });
    }

    // For both `script` and `style`, the content is inside a TextNode
    let content = node
        .children
        .get(0)
        .and_then(|child| match child {
            Node::TextNode(text) => Some(*text),
            _ => None,
        })
        .unwrap_or("");

    if is_script {
        let is_setup = starting_tag
            .attributes
            .iter()
            .any(|attr| matches!(attr, HtmlAttribute::Regular { name: "setup", .. }));

        // TODO What should be done if for some reason `content` is an empty string?
        // This means that either parsing failed or the content is really empty
        // Maybe it should be checked in analyzer??

        return SfcBlock::Script(SfcScriptBlock {
            lang: lang.unwrap_or("js"),
            content,
            is_setup,
        });
    }

    if is_style {
        let is_scoped = starting_tag
            .attributes
            .iter()
            .any(|attr| matches!(attr, HtmlAttribute::Regular { name: "scoped", .. }));

        return SfcBlock::Style(SfcStyleBlock {
            lang: lang.unwrap_or("css"),
            content,
            is_scoped,
        });
    }

    unreachable!("All SFC block variants were handled")
}
