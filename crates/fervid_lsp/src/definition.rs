use serde_json::json;
use tower_lsp::{
    jsonrpc::{Error, ErrorCode, Result},
    lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url},
};

use crate::{Backend, nuxt::resolver::resolve_from_nuxt_dir, utils::{extract_token_at, normalize_path, pick_workspace_root, uri_to_path}};

pub fn goto_definition(
    backend: &Backend,
    position: &Position,
    uri: &Url,
) -> Result<Option<GotoDefinitionResponse>> {
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

    let token = extract_token_at(&rope, cursor_char);
    if token.is_empty() {
        return Ok(None);
    }

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

    let Some(import_path) = globals.components.get(&token) else {
        return Ok(None);
    };

    let target_path = normalize_path(resolve_from_nuxt_dir(root, import_path));

    let target_uri = Url::from_file_path(&target_path).map_err(|_| Error {
        code: ErrorCode::ServerError(-34010),
        message: "Unable to convert file path to URI".into(),
        data: Some(json!(target_path)),
    })?;

    let loc = Location::new(
        target_uri,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
    );

    Ok(Some(GotoDefinitionResponse::Scalar(loc)))
}
