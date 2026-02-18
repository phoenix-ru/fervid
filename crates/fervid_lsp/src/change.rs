use std::sync::Arc;

use crate::{
    offset_to_position,
    structs::{Backend, DocumentAnalysis, TextDocumentItem},
};
use fervid_core::{compute_scope_id, SfcDescriptor};
use fervid_parser::{ParseError, SfcParser};
use fervid_transform::{transform_sfc, TransformSfcOptions};
use ropey::Rope;
use swc_core::common::Spanned;
use tokio::task;
use tower_lsp::lsp_types::{Diagnostic, Range};

pub async fn on_change<'a>(backend: &Backend, params: TextDocumentItem<'a>) {
    let uri_key = params.uri.to_string();

    // 1. Store rope immediately (fast path)
    let rope = Rope::from_str(params.text);
    backend.document_map.insert(uri_key.clone(), rope.clone());

    // 2. Parse SFC (sync) and publish diagnostics immediately
    let (sfc_opt, sfc_parsing_errors) = parse(params.text);

    let diagnostics = sfc_parsing_errors
        .into_iter()
        .filter_map(|parse_error| {
            let message = parse_error.kind.to_string();
            let span = parse_error.span;
            let start_position = offset_to_position(span.lo.0 as usize, &rope)?;
            let end_position = offset_to_position(span.hi.0 as usize, &rope)?;
            Some(Diagnostic::new_simple(
                Range::new(start_position, end_position),
                message,
            ))
        })
        .collect::<Vec<_>>();

    backend
        .client
        .publish_diagnostics(params.uri.clone(), diagnostics, params.version)
        .await;

    // 3.1. If parse failed, do NOT overwrite previous ast/analysis
    let Some(sfc) = sfc_opt else {
        return;
    };

    // 3.2. Store the initial unprocessed descriptor for finding cursor locations
    backend.ast_map.insert(uri_key.clone(), sfc.clone());

    // 4. Do SFC transformation in the background to not block CPU
    let client = backend.client.clone();
    let analysis_map = backend.analysis_map.clone();
    let uri = params.uri.clone();
    let version = params.version;
    let file_hash = compute_scope_id(params.text);

    task::spawn(async move {
        let mut transform_errors = Vec::new();
        let transform_options = TransformSfcOptions {
            is_prod: true,
            is_ce: false,
            props_destructure: fervid_transform::PropsDestructureConfig::True,
            scope_id: &file_hash,
            filename: uri.as_str(),
            transform_asset_urls: fervid_transform::TransformAssetUrlsConfig::Disabled,
        };
        let transform_result = transform_sfc(sfc, transform_options, &mut transform_errors);

        if !transform_errors.is_empty() {
            let diagnostics = transform_errors
                .into_iter()
                .filter_map(|transform_error| {
                    let message = transform_error.to_string();
                    let span = transform_error.span();
                    let start_position = offset_to_position(span.lo.0 as usize, &rope)?;
                    let end_position = offset_to_position(span.hi.0 as usize, &rope)?;
                    Some(Diagnostic::new_simple(
                        Range::new(start_position, end_position),
                        message,
                    ))
                })
                .collect::<Vec<_>>();

            client
                .publish_diagnostics(params.uri.clone(), diagnostics, params.version)
                .await;
        }

        let analysis = DocumentAnalysis {
            version,
            bindings: Arc::new(transform_result.bindings_helper),
            template_block: transform_result.template_block,
        };

        analysis_map.insert(uri.to_string(), Arc::new(analysis));

        // backend.semantic_token_map
        //     .insert(params.uri.to_string(), semantic_tokens);
    });
}

fn parse(input: &str) -> (Option<SfcDescriptor>, Vec<ParseError>) {
    let mut sfc_parsing_errors = Vec::new();

    let mut parser = SfcParser::new(input, &mut sfc_parsing_errors);
    let sfc = match parser.parse_sfc() {
        Ok(sfc) => Some(sfc),
        Err(e) => {
            sfc_parsing_errors.push(e);
            None
        }
    };

    (sfc, sfc_parsing_errors)
}
