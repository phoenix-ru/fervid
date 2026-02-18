use fervid_core::FervidAtom;
use fervid_transform::BindingsHelper;
use fxhash::FxHashMap;
use serde_json::json;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{CompletionItem, InsertTextFormat, Position, Url};

use crate::utils::extract_prefix;
use crate::{
    nuxt::NuxtGlobals,
    utils::{pick_workspace_root, uri_to_path},
    Backend,
};

pub struct FervidCompletionItem {
    name: FervidAtom,
    kind: CompletionItemKind,
    source_kind: CompletionSourceKind,
    source_origin: Option<FervidAtom>,
}

#[derive(Clone, Copy)]
pub enum CompletionItemKind {
    Component,
    Import,
    #[allow(dead_code)]
    Prop,
    Variable,
}

pub enum CompletionSourceKind {
    /// Completions retrieved from Nuxt metadata (auto-imports and components)
    Nuxt,
    /// Completions retrieved from the current file
    #[allow(dead_code)]
    Local,
}

pub fn provide_completions(
    backend: &Backend,
    position: &Position,
    uri: &Url,
) -> Result<Vec<FervidCompletionItem>> {
    let uri_key = uri.to_string();

    let rope = backend.document_map.get(&uri_key).ok_or_else(|| Error {
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

    let doc_path = uri_to_path(uri).ok_or(Error {
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

    let mut result = FxHashMap::default();

    // Global completions from Nuxt
    let globals = backend.nuxt_globals.get(&root_key).ok_or_else(|| Error {
        code: ErrorCode::ServerError(-33012),
        message: "Unable to find workspace root data".into(),
        data: Some(json!(root_key)),
    })?;
    completion_from_globals(&globals, &prefix, &mut result);

    // Local document completions
    if let Some(analysis) = backend.analysis_map.get(&uri_key) {
        completion_from_setup_bindings(&analysis.bindings, &prefix, &mut result);
    }

    Ok(result.into_values().collect())
}

/// Naive implementation of completions using prefix matching from globals.
/// It is naive in a sense that it does not consider the surrounding context (are we inside `<script>` or `<template>`)
pub fn completion_from_globals(
    globals: &NuxtGlobals,
    prefix: &str,
    out: &mut FxHashMap<FervidAtom, FervidCompletionItem>,
) {
    // Components
    for (name, target) in globals.components.iter() {
        if prefix.is_empty() || name.starts_with(prefix) {
            out.insert(
                name.to_owned(),
                FervidCompletionItem {
                    name: name.as_str().into(),
                    kind: CompletionItemKind::Component,
                    source_kind: CompletionSourceKind::Nuxt,
                    source_origin: Some(target.spec.to_owned()),
                },
            );
        }
    }

    // Imports
    for (name, target) in globals.imports.iter() {
        if prefix.is_empty() || name.starts_with(prefix) {
            out.insert(
                name.to_owned(),
                FervidCompletionItem {
                    name: name.to_owned(),
                    kind: CompletionItemKind::Import,
                    source_kind: CompletionSourceKind::Nuxt,
                    source_origin: Some(target.spec.to_owned()),
                },
            );
        }
    }
}

pub fn completion_from_setup_bindings(
    bindings: &BindingsHelper,
    prefix: &str,
    out: &mut FxHashMap<FervidAtom, FervidCompletionItem>,
) {
    for b in bindings.setup_bindings.iter() {
        let name = &b.sym; // FervidAtom
        if prefix.is_empty() || name.as_ref().starts_with(prefix) {
            out.insert(
                name.to_owned(),
                FervidCompletionItem {
                    name: name.to_owned(),
                    kind: CompletionItemKind::Variable,
                    source_kind: CompletionSourceKind::Local,
                    source_origin: None,
                },
            );
        }
    }
}

impl From<FervidCompletionItem> for tower_lsp::lsp_types::CompletionItem {
    fn from(val: FervidCompletionItem) -> Self {
        let name = val.name.to_string();
        let label = name.clone();
        let detail = Some(format_detail(val.source_kind, val.kind, val.source_origin));

        match val.kind {
            CompletionItemKind::Component => CompletionItem {
                label,
                kind: Some(tower_lsp::lsp_types::CompletionItemKind::CLASS),
                insert_text: Some(name),
                detail,
                ..Default::default()
            },
            CompletionItemKind::Prop => CompletionItem {
                label,
                kind: Some(tower_lsp::lsp_types::CompletionItemKind::VARIABLE),
                insert_text: Some(format!("{}=\"$1\"", name)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                detail,
                ..Default::default()
            },
            CompletionItemKind::Import => CompletionItem {
                label,
                kind: Some(tower_lsp::lsp_types::CompletionItemKind::FUNCTION),
                insert_text: Some(name),
                detail,
                ..Default::default()
            },
            CompletionItemKind::Variable => CompletionItem {
                label,
                kind: Some(tower_lsp::lsp_types::CompletionItemKind::VARIABLE),
                insert_text: Some(name),
                detail,
                ..Default::default()
            },
        }
    }
}

fn format_detail(
    source_kind: CompletionSourceKind,
    completion_kind: CompletionItemKind,
    source_origin: Option<FervidAtom>,
) -> String {
    let prefix = format_source_prefix(source_kind, completion_kind);
    if let Some(source_origin) = source_origin {
        format!("{prefix} from {source_origin}")
    } else {
        prefix.to_string()
    }
}

fn format_source_prefix(
    source_kind: CompletionSourceKind,
    completion_kind: CompletionItemKind,
) -> &'static str {
    match (source_kind, completion_kind) {
        (CompletionSourceKind::Local, CompletionItemKind::Component) => "Local component",
        (CompletionSourceKind::Local, CompletionItemKind::Import) => "Import",
        (CompletionSourceKind::Local, CompletionItemKind::Prop) => "Prop",
        (CompletionSourceKind::Local, CompletionItemKind::Variable) => "Variable",
        (CompletionSourceKind::Nuxt, CompletionItemKind::Component) => "Nuxt component",
        (CompletionSourceKind::Nuxt, CompletionItemKind::Import) => "Nuxt auto-import",
        (CompletionSourceKind::Nuxt, CompletionItemKind::Prop) => "Prop",
        (CompletionSourceKind::Nuxt, CompletionItemKind::Variable) => "Variable",
    }
}
