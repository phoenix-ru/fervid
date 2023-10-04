use fervid_core::{ElementNode, FervidAtom, Interpolation, Node, PatchHints, SfcTemplateBlock};
use swc_core::{
    common::{BytePos, Span},
    ecma::atoms::js_word,
};
use swc_ecma_parser::{Syntax, TsConfig};
use swc_html_ast::{Child, Element, Text};

use crate::{common::process_element_starting_tag, error::ParseError, script::parse_expr};

// Default patterns, this will be moved to the config area in the future
const INTERPOLATION_START_PAT: &str = "{{";
const INTERPOLATION_END_PAT: &str = "}}";

pub fn parse_template_to_ir(
    root_element: Element,
    errors: &mut Vec<ParseError>,
) -> Option<SfcTemplateBlock> {
    // TODO Errors in template

    let lang = root_element
        .attributes
        .iter()
        .find_map(|attr| {
            if attr.name == js_word!("lang") {
                Some(attr.name.to_owned())
            } else {
                None
            }
        })
        .unwrap_or_else(|| js_word!("html"));

    // <template> technically has a `content`
    let children: Vec<Child> = root_element
        .content
        .map(|c| c.children)
        .unwrap_or_else(|| root_element.children);

    Some(SfcTemplateBlock {
        lang,
        roots: process_element_children(children, errors),
        span: root_element.span,
    })
}

fn process_element(element: Element, errors: &mut Vec<ParseError>) -> Node {
    let children: Vec<Child> = element
        .content
        .map(|c| c.children)
        .unwrap_or_else(|| element.children);

    Node::Element(ElementNode {
        kind: fervid_core::ElementKind::Element,
        starting_tag: process_element_starting_tag(element.tag_name, element.attributes, errors),
        children: process_element_children(children, errors),
        template_scope: 0,
        patch_hints: PatchHints::default(),
        span: element.span,
    })
}

fn process_element_children(children: Vec<Child>, errors: &mut Vec<ParseError>) -> Vec<Node> {
    let mut out = Vec::with_capacity(children.len());

    for child in children {
        match child {
            Child::DocumentType(_) => unimplemented!("Doctype is unsupported"),
            Child::Element(element) => out.push(process_element(element, errors)),
            Child::Text(text) => process_text(
                text,
                &mut out,
                errors,
                INTERPOLATION_START_PAT,
                INTERPOLATION_END_PAT,
            ),
            Child::Comment(comment) => out.push(Node::Comment(comment.data, comment.span)),
        }
    }

    out
}

/// Separates a raw text into `Node::Text`s and `Node::Interpolation`s
fn process_text(
    text: Text,
    out: &mut Vec<Node>,
    errors: &mut Vec<ParseError>,
    interpolation_start_pat: &str,
    interpolation_end_pat: &str,
) {
    let Text { span, data, .. } = text;
    let raw: &str = &data;
    let interpolation_start_pat_len = interpolation_start_pat.len();
    let interpolation_end_pat_len = interpolation_end_pat.len();

    // let mut curr_text = "";
    // let mut curr_text_start_idx = 0;
    // let mut curr_text_end_idx = 0;
    let mut text_start_idx = 0;

    // Find interpolation start - `{{` by default
    for (match_idx, _) in raw.match_indices(interpolation_start_pat) {
        let interpolation_start_idx = match_idx + interpolation_start_pat_len;

        // Find interpolation end - `}}` by default
        let Some(interpolation_end_idx) =
            raw[interpolation_start_idx..].find(interpolation_end_pat)
        else {
            continue;
        };

        // Offset, because we did offset while `find`ing previously
        let interpolation_end_idx = interpolation_end_idx + interpolation_start_idx;

        // Add any previous text
        if text_start_idx < match_idx {
            let offset = span.lo.0 + text_start_idx as u32;
            let text = &raw[text_start_idx..match_idx];
            let text_span = Span {
                lo: BytePos(offset),
                hi: BytePos(offset + text.len() as u32),
                ctxt: Default::default(),
            };

            out.push(Node::Text(FervidAtom::from(text), text_span));
        }
        text_start_idx = interpolation_end_idx + interpolation_end_pat_len;

        // Get the interpolation &str
        let interpolation = &raw[interpolation_start_idx..interpolation_end_idx];

        // Span stuff
        let offset = span.lo.0 + interpolation_start_idx as u32;
        let interpolation_span = Span::new(
            BytePos(offset),
            BytePos(offset + interpolation.len() as u32),
            Default::default(),
        );

        match parse_expr(
            interpolation,
            Syntax::Typescript(TsConfig::default()),
            interpolation_span,
        ) {
            Ok(parsed_interpolation) => out.push(Node::Interpolation(Interpolation {
                value: parsed_interpolation,
                template_scope: 0,
                patch_flag: false,
            })),
            Err(expr_err) => errors.push(expr_err.into()),
        }
    }

    // Add the remaining text if any
    if text_start_idx < raw.len() {
        let text_span = Span::new(
            BytePos(span.lo.0 + text_start_idx as u32),
            span.hi,
            span.ctxt,
        );
        out.push(Node::Text(
            FervidAtom::from(&raw[text_start_idx..]),
            text_span,
        ));
    }

    // let mut remaining = raw;
    // let mut remaining_start_idx = 0;

    // // Find interpolation start - `{{` by default
    // for (match_idx, _) in raw.match_indices(interpolation_start_pat) {
    //     let interpolation_start_idx = match_idx + interpolation_start_pat_len;

    //     // Find interpolation end - `}}` by default
    //     if let Some(interpolation_end_idx) =
    //         raw[interpolation_start_idx..].find(interpolation_end_pat)
    //     {
    //         // Offset
    //         let interpolation_end_idx = interpolation_end_idx + interpolation_start_idx;
    //         let interpolation = &raw[interpolation_start_idx..interpolation_end_idx];

    //         // Include everything prior to interpolation

    //         // Span stuff
    //         let offset = span.lo.0 + interpolation_start_idx as u32;
    //         let interpolation_span = Span::new(
    //             BytePos(offset),
    //             BytePos(offset + interpolation.len() as u32),
    //             Default::default(),
    //         );

    //         match parse_expr(
    //             interpolation,
    //             Syntax::Typescript(TsConfig::default()),
    //             interpolation_span,
    //         ) {
    //             Ok(parsed_interpolation) => out.push(Node::Interpolation(Interpolation {
    //                 value: parsed_interpolation,
    //                 template_scope: 0,
    //                 patch_flag: false,
    //             })),
    //             Err(expr_err) => errors.push(expr_err.into()),
    //         }

    //         // Advance
    //         remaining_start_idx = interpolation_end_idx + interpolation_end_pat_len;
    //         remaining = &raw[remaining_start_idx..];
    //     }
    // }

    // if !remaining.is_empty() {
    //     let offset = span.lo.0 + remaining_start_idx as u32;
    //     let new_span = Span::new(
    //         BytePos(offset),
    //         BytePos(offset + remaining.len() as u32),
    //         Default::default(),
    //     );
    //     out.push(Node::Text(FervidAtom::from(remaining), new_span));
    // }
}
