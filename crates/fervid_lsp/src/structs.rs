use crate::nuxt::{NuxtGlobals, NuxtInfo};
use dashmap::DashMap;
use fervid_core::{SfcDescriptor, SfcTemplateBlock};
use fervid_transform::BindingsHelper;
use ropey::Rope;
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tower_lsp::{Client, lsp_types::Url};

type WorkspaceKey = String;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,

    // TODO: We should probably store information in Workspace structs instead
    /// Map for storing the raw initial AST by file URI, where AST is the full SFC descriptor
    pub ast_map: Arc<DashMap<String, SfcDescriptor>>,

    /// Map for storing the results of document analysis, such as bindings, etc.
    pub analysis_map: Arc<DashMap<String, Arc<DocumentAnalysis>>>,

    /// Map for storing the contents of the document
    pub document_map: DashMap<String, Rope>,

    /// Workspace directories of the project
    pub workspace_roots: RwLock<Vec<PathBuf>>,

    // semantic_map: DashMap<String, Semantic>,
    // semantic_token_map: DashMap<String, Vec<ImCompleteSemanticToken>>,

    // Nuxt-specific
    pub nuxt_info: DashMap<WorkspaceKey, Arc<NuxtInfo>>,
    pub nuxt_globals: DashMap<WorkspaceKey, Arc<NuxtGlobals>>,
}

#[derive(Debug, Clone)]
pub struct DocumentAnalysis {
    #[allow(unused)]
    pub version: Option<i32>,
    pub bindings: Arc<BindingsHelper>,
    #[allow(unused)]
    pub template_block: Option<SfcTemplateBlock>,
}

pub struct TextDocumentItem<'a> {
    pub uri: Url,
    pub text: &'a str,
    pub version: Option<i32>,
}
