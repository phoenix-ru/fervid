use std::path::{Component, PathBuf};

use ropey::Rope;
use tower_lsp::lsp_types::{Url, WorkspaceFolder};

pub fn workspace_uri_to_path(workspace_folder: &WorkspaceFolder) -> Option<std::path::PathBuf> {
    uri_to_path(&workspace_folder.uri)
}

pub fn uri_to_path(uri: &Url) -> Option<std::path::PathBuf> {
    if uri.scheme() != "file" {
        return None;
    }

    uri.to_file_path().ok()
}

pub fn pick_workspace_root<'a>(roots: &'a [PathBuf], file: &PathBuf) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|root_path| file.starts_with(root_path))
        .max_by_key(|root_path| root_path.as_os_str().len())
}

pub fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '-'
}

/// Extracts prefix from the current cursor.
/// Example: if cursor | is at some|thing, the extracted prefix is `some`
pub fn extract_prefix(rope: &Rope, cursor_char: usize) -> String {
    let mut start = cursor_char;

    while start > 0 {
        let c = rope.char(start - 1);
        if is_ident_char(c) {
            start -= 1;
        } else {
            break;
        }
    }

    rope.slice(start..cursor_char).to_string()
}

/// Extracts the full token under the current cursor.
/// Example: if cursor | is at some|thing, the extracted token is `something`
pub fn extract_token_at(rope: &ropey::Rope, cursor_char: usize) -> String {
    let len = rope.len_chars();
    if cursor_char > len {
        return String::new();
    }

    // If cursor is at end of token, step left once (nice UX)
    let mut i = cursor_char;
    if i > 0 && (i == len || !is_ident_char(rope.char(i))) && is_ident_char(rope.char(i - 1)) {
        i -= 1;
    }

    let mut start = i;
    while start > 0 && is_ident_char(rope.char(start - 1)) {
        start -= 1;
    }

    let mut end = i + 1;
    while end < len && is_ident_char(rope.char(end)) {
        end += 1;
    }

    rope.slice(start..end).to_string()
}

/// Naive implementation of path normalization without resorting to `canonicalize`
/// and fs access
pub fn normalize_path(p: PathBuf) -> PathBuf {
    let mut stack: Vec<std::ffi::OsString> = Vec::new();

    for c in p.components() {
        match c {
            Component::ParentDir => {
                stack.pop();
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {
                stack.clear();
                stack.push(c.as_os_str().to_os_string());
            }
            Component::Normal(s) => stack.push(s.to_os_string()),
        }
    }

    let mut out = PathBuf::new();
    for s in stack {
        out.push(s);
    }
    out
}
