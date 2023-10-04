use fervid_core::{FervidAtom, SfcCustomBlock, SfcDescriptor, SfcStyleBlock};
use swc_core::{
    common::{BytePos, Span, DUMMY_SP},
    ecma::atoms::js_word,
};
use swc_ecma_parser::StringInput;
use swc_html_ast::{Child, DocumentFragment, DocumentMode, Element, Namespace};
use swc_html_parser::{
    lexer::Lexer,
    parser::{Parser, ParserConfig},
};

use crate::{
    common::process_element_starting_tag,
    error::{ParseError, ParseErrorKind},
    script::parse_sfc_script_element,
    template::parse_template_to_ir,
};

pub fn parse_sfc(
    input: &str,
    errors: &mut Vec<ParseError>,
) -> Result<SfcDescriptor, ParseError> {
    // TODO When should it fail? What do we do with errors?..
    // I was thinking of 4 strategies:
    // `HARD_FAIL_ON_ERROR` (any error = fail (incl. recoverable), stop at the first error),
    // `SOFT_REPORT_ALL` (any error = fail (incl. recoverable), continue as far as possible and report after),
    // `SOFT_RECOVER_SAFE` (try to ignore recoverable, report the rest),
    // `SOFT_RECOVER_UNSAFE` (ignore as much as is possible, but still report).

    let mut html_parse_errors = Vec::new();
    let parsed_html = parse_html_document_fragment(input, &mut html_parse_errors).map_err(|e| {
        let kind = e.into_inner().1;

        ParseError {
            kind: ParseErrorKind::InvalidHtml(kind),
            span: Span {
                lo: BytePos(1),
                hi: BytePos(input.len() as u32),
                ctxt: Default::default(),
            },
        }
    })?;

    // TODO Check for serious errors (because it may parse regardless) ???
    errors.reserve(html_parse_errors.len());
    for html_parse_error in html_parse_errors {
        let e = html_parse_error.into_inner();
        errors.push(ParseError { kind: ParseErrorKind::InvalidHtml(e.1), span: e.0 })
    }

    let mut sfc_descriptor = SfcDescriptor::default();

    for root_node in parsed_html.children.into_iter() {
        // Only root elements are supported
        let Child::Element(root_element) = root_node else {
            continue;
        };

        let tag_name = &root_element.tag_name;

        if tag_name.eq(&js_word!("template")) {
            if sfc_descriptor.template.is_some() {
                // Duplicate template, bail
                // TODO Error
                continue;
            }

            let template_result = parse_template_to_ir(root_element, errors);
            if template_result.is_none() {
                // TODO Error
                continue;
            };

            sfc_descriptor.template = template_result;
        } else if tag_name.eq(&js_word!("script")) {
            let Some(sfc_script_block) = parse_sfc_script_element(root_element) else {
                continue;
            };

            if sfc_script_block.is_setup {
                // Check if already present
                if sfc_descriptor.script_setup.is_some() {
                    // Duplicate script setup, bail
                    // TODO Error
                    continue;
                }
                sfc_descriptor.script_setup = Some(sfc_script_block);
            } else {
                // Check if already present
                if sfc_descriptor.script_legacy.is_some() {
                    // Duplicate script legacy, bail
                    // TODO Error
                    continue;
                }
                sfc_descriptor.script_legacy = Some(sfc_script_block);
            }
        } else if tag_name.eq(&js_word!("style")) {
            // Check that `<style>` is not empty
            let Some(Child::Text(style_content)) = root_element.children.get(0) else {
                continue;
            };

            let mut lang = FervidAtom::from("css");
            let mut is_scoped = false;

            for attr in root_element.attributes.into_iter() {
                if attr.name.eq("lang") {
                    let Some(attr_val) = attr.value else {
                        continue;
                    };

                    lang = attr_val;
                } else if attr.name.eq("scoped") {
                    is_scoped = true;
                }
            }

            sfc_descriptor.styles.push(SfcStyleBlock {
                lang,
                content: style_content.data.to_owned(),
                is_scoped,
            })
        } else {
            let starting_tag = process_element_starting_tag(
                root_element.tag_name,
                root_element.attributes,
                errors,
            );

            // TODO Use span of contents, not the full span
            let span = root_element.span;

            sfc_descriptor.custom_blocks.push(SfcCustomBlock {
                starting_tag,
                content: FervidAtom::from(
                    &input[(span.lo.0 - 1) as usize..(span.hi.0 - 1) as usize],
                ),
            })
        }
    }

    Ok(sfc_descriptor)
}

/// Adapted from `swc_html_parser`
#[inline]
pub fn parse_html_document_fragment(
    input: &str,
    errors: &mut Vec<swc_html_parser::error::Error>,
) -> Result<DocumentFragment, swc_html_parser::error::Error> {
    let lexer = Lexer::new(StringInput::new(
        input,
        BytePos(1),
        BytePos(input.len() as u32),
    ));

    let parser_config = ParserConfig {
        scripting_enabled: false,
        iframe_srcdoc: false,
    };
    let mut parser = Parser::new(lexer, parser_config);

    let ctx_element = Element {
        span: DUMMY_SP,
        tag_name: js_word!("div"),
        namespace: Namespace::HTML,
        attributes: vec![],
        children: vec![],
        content: None,
        is_self_closing: false,
    };

    let result = parser.parse_document_fragment(ctx_element, DocumentMode::NoQuirks, None);

    errors.extend(parser.take_errors());

    result
}
