use fervid_core::{
    fervid_atom, AttributeOrBinding, FervidAtom, SfcCustomBlock, SfcDescriptor, SfcStyleBlock,
    StartingTag,
};
use swc_core::common::{BytePos, Span, DUMMY_SP};
use swc_ecma_parser::StringInput;
use swc_html_ast::{Child, DocumentFragment, DocumentMode, Element, Namespace};
use swc_html_parser::{
    lexer::Lexer,
    parser::{Parser, ParserConfig},
};

use crate::{
    error::{ParseError, ParseErrorKind},
    script::parse_sfc_script_element,
    SfcParser,
};

type SwcHtmlParserError = swc_html_parser::error::Error;

impl SfcParser<'_, '_, '_> {
    /// Parses `self.input` as an SFC, producing an `SfcDescriptor`.
    /// When `Err(ParseError)` is returned, that means unrecoverable error was discovered.
    pub fn parse_sfc(&mut self) -> Result<SfcDescriptor, ParseError> {
        let parsed_html = self.parse_html_document_fragment().map_err(|e| {
            let kind = e.into_inner().1;

            ParseError {
                kind: ParseErrorKind::InvalidHtml(Box::new(kind)),
                span: Span {
                    lo: BytePos(1),
                    hi: BytePos(self.input.len() as u32),
                    ctxt: Default::default(),
                },
            }
        })?;

        let mut sfc_descriptor = SfcDescriptor::default();

        macro_rules! report_error {
            ($kind: ident, $span: expr) => {
                self.errors.push(ParseError { kind: ParseErrorKind::$kind, span: $span });
            };
        }

        for root_node in parsed_html.children.into_iter() {
            // Only root elements are supported
            let Child::Element(root_element) = root_node else {
                continue;
            };

            let tag_name = &root_element.tag_name;
            let root_node_span = root_element.span;

            if tag_name.eq("template") {
                // Check duplicate
                if sfc_descriptor.template.is_some() {
                    report_error!(DuplicateTemplate, root_node_span);
                    continue;
                }

                let template_result = self.parse_template_to_ir(root_element);
                if template_result.is_none() {
                    // TODO Error
                    continue;
                };

                sfc_descriptor.template = template_result;
            } else if tag_name.eq("script") {
                let Some(sfc_script_block) = parse_sfc_script_element(root_element) else {
                    continue;
                };

                if sfc_script_block.is_setup {
                    // Check if already present
                    if sfc_descriptor.script_setup.is_some() {
                        report_error!(DuplicateScriptSetup, root_node_span);
                        continue;
                    }
                    sfc_descriptor.script_setup = Some(sfc_script_block);
                } else {
                    // Check if already present
                    if sfc_descriptor.script_legacy.is_some() {
                        report_error!(DuplicateScriptOptions, root_node_span);
                        continue;
                    }
                    sfc_descriptor.script_legacy = Some(sfc_script_block);
                }
            } else if tag_name.eq("style") {
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
                    span: style_content.span
                })
            } else {
                let attributes = root_element
                    .attributes
                    .into_iter()
                    .map(|attr| AttributeOrBinding::RegularAttribute {
                        name: attr.name,
                        value: attr.value.unwrap_or_else(|| fervid_atom!("")),
                    })
                    .collect();

                let starting_tag = StartingTag {
                    tag_name: root_element.tag_name,
                    attributes,
                    directives: None,
                };

                // TODO Use span of contents, not the full span
                let span = root_element.span;

                sfc_descriptor.custom_blocks.push(SfcCustomBlock {
                    starting_tag,
                    content: FervidAtom::from(
                        &self.input[(span.lo.0 - 1) as usize..(span.hi.0 - 1) as usize],
                    ),
                })
            }
        }

        Ok(sfc_descriptor)
    }

    /// Adapted from `swc_html_parser`
    #[inline]
    pub fn parse_html_document_fragment(&mut self) -> Result<DocumentFragment, SwcHtmlParserError> {
        let lexer = Lexer::new(StringInput::new(
            self.input,
            BytePos(1),
            BytePos(self.input.len() as u32),
        ));

        let parser_config = ParserConfig {
            scripting_enabled: false,
            iframe_srcdoc: false,
        };
        let mut parser = Parser::new(lexer, parser_config);

        let ctx_element = Element {
            span: DUMMY_SP,
            tag_name: fervid_atom!("div"),
            namespace: Namespace::HTML,
            attributes: vec![],
            children: vec![],
            content: None,
            is_self_closing: false,
        };

        let result = parser.parse_document_fragment(ctx_element, DocumentMode::NoQuirks, None);

        let html_parse_errors = parser.take_errors();

        // TODO Check for serious errors (because it may parse regardless) ???
        self.errors.reserve(html_parse_errors.len());
        for html_parse_error in html_parse_errors {
            let e = html_parse_error.into_inner();
            self.errors.push(ParseError {
                kind: ParseErrorKind::InvalidHtml(Box::new(e.1)),
                span: e.0,
            })
        }

        result
    }
}
