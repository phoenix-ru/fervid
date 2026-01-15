use fervid_core::FervidAtom;
use serde_json::json;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{Position, Url};

use crate::utils::extract_prefix;
use crate::{
    nuxt::NuxtGlobals,
    utils::{pick_workspace_root, uri_to_path},
    Backend,
};

pub enum FervidCompletionItem {
    Component { name: FervidAtom },
    Prop { name: FervidAtom },
    Import { name: FervidAtom },
}

pub fn provide_completions(
    backend: &Backend,
    position: &Position,
    uri: &Url,
) -> Result<Vec<FervidCompletionItem>> {
    let rope = backend
        .document_map
        .get(&uri.to_string())
        .ok_or_else(|| Error {
            code: ErrorCode::InvalidParams,
            message: "Unable to find contents of a file".into(),
            data: Some(json!(uri.as_str())),
        })?;

    // TODO: this is not UTF-16 safe, LSP seems to use UTF-16, but ropey seemingly uses chars
    let line_start_char = rope
        .try_line_to_char(position.line as usize)
        .map_err(|_| Error {
            code: ErrorCode::InvalidParams,
            message: "Unable to convert line to character index".into(),
            data: Some(json!(uri.as_str())),
        })?;
    let cursor_char = line_start_char + position.character as usize;

    let prefix = extract_prefix(&rope, cursor_char);

    let doc_path = uri_to_path(&uri).ok_or(Error {
        code: ErrorCode::InvalidParams,
        message: "Unable to resolve completion path as file".into(),
        data: Some(json!(uri.as_str())),
    })?;

    let roots = backend.workspace_roots.try_read().map_err(|_| Error {
        code: ErrorCode::ServerError(-33010),
        message: "Unable to acquire lock on workspace_roots".into(),
        data: None,
    })?;

    let root = pick_workspace_root(&roots, &doc_path).ok_or_else(|| Error {
        code: ErrorCode::ServerError(-33011),
        message: "Unable to pick workspace root for file".into(),
        data: Some(json!(doc_path)),
    })?;

    let root_key = root.to_string_lossy().to_string();

    let globals = backend.nuxt_globals.get(&root_key).ok_or_else(|| Error {
        code: ErrorCode::ServerError(-33012),
        message: "Unable to find workspace root data".into(),
        data: Some(json!(root_key)),
    })?;

    Ok(completion_from_globals(&globals, &prefix))
}

/// Naive implementation of completions using prefix matching from globals.
/// It is naive in a sense that it does not consider the surrounding context (are we inside `<script>` or `<template>`)
pub fn completion_from_globals(globals: &NuxtGlobals, prefix: &str) -> Vec<FervidCompletionItem> {
    let mut result = Vec::new();

    // Components
    for name in globals.components.keys() {
        if prefix.is_empty() || name.starts_with(prefix) {
            result.push(FervidCompletionItem::Component {
                name: name.as_str().into(),
            });
        }
    }

    // Imports
    for name in globals.imports.keys() {
        if prefix.is_empty() || name.starts_with(prefix) {
            result.push(FervidCompletionItem::Import {
                name: name.as_str().into(),
            });
        }
    }

    result
}
