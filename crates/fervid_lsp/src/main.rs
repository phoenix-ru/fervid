use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use log::debug;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Notification;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService, Server};

use crate::change::on_change;
use crate::completion::provide_completions;
use crate::definition::goto_definition;
use crate::nuxt::loader::load_nuxt_for_workspaces;
use crate::structs::{Backend, TextDocumentItem};
use crate::utils::workspace_uri_to_path;

mod change;
mod completion;
mod definition;
mod nuxt;
mod structs;
mod utils;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(workspace_folders) = params.workspace_folders {
            load_nuxt_for_workspaces(self, &workspace_folders).await;

            // Save workspace roots
            let roots = workspace_folders
                .into_iter()
                .filter_map(|workspace_folder| workspace_uri_to_path(&workspace_folder))
                .collect();

            match self.workspace_roots.try_write() {
                Ok(mut lock) => {
                    *lock = roots;
                }
                Err(error) => {
                    log::error!("Error acquiring lock on workspace_roots {}", error);
                }
            }
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            offset_encoding: None,
            capabilities: ServerCapabilities {
                // inlay_hint_provider: Some(OneOf::Left(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    completion_item: None,
                }),
                // execute_command_provider: Some(ExecuteCommandOptions {
                //     commands: vec!["dummy.do_something".to_string()],
                //     work_done_progress_options: Default::default(),
                // }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                // semantic_tokens_provider: Some(
                //     SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                //         SemanticTokensRegistrationOptions {
                //             text_document_registration_options: {
                //                 TextDocumentRegistrationOptions {
                //                     document_selector: Some(vec![DocumentFilter {
                //                         language: Some("nrs".to_string()),
                //                         scheme: Some("file".to_string()),
                //                         pattern: None,
                //                     }]),
                //                 }
                //             },
                //             semantic_tokens_options: SemanticTokensOptions {
                //                 work_done_progress_options: WorkDoneProgressOptions::default(),
                //                 legend: SemanticTokensLegend {
                //                     token_types: LEGEND_TYPE.into(),
                //                     token_modifiers: vec![],
                //                 },
                //                 range: Some(true),
                //                 full: Some(SemanticTokensFullOptions::Bool(true)),
                //             },
                //             static_registration_options: StaticRegistrationOptions::default(),
                //         },
                //     ),
                // ),
                // definition: Some(GotoCapability::default()),
                definition_provider: Some(OneOf::Left(true)),
                // references_provider: Some(OneOf::Left(true)),
                // rename_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }
    async fn initialized(&self, _: InitializedParams) {
        debug!("initialized!");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        debug!("file opened");
        on_change(
            self,
            TextDocumentItem {
                uri: params.text_document.uri,
                text: &params.text_document.text,
                version: Some(params.text_document.version),
            },
        )
        .await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        on_change(
            self,
            TextDocumentItem {
                text: &params.content_changes[0].text,
                uri: params.text_document.uri,
                version: Some(params.text_document.version),
            },
        )
        .await
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            let item = TextDocumentItem {
                uri: params.text_document.uri,
                text: &text,
                version: None,
            };
            on_change(self, item).await;
            _ = self.client.semantic_tokens_refresh().await;
        }
        debug!("file saved!");
    }
    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        debug!("file closed!");
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        goto_definition(self, &position, &uri)
    }

    // async fn goto_definition(
    //     &self,
    //     params: GotoDefinitionParams,
    // ) -> Result<Option<GotoDefinitionResponse>> {
    //     let definition = || -> Option<GotoDefinitionResponse> {
    //         let uri = params.text_document_position_params.text_document.uri;
    //         let semantic = self.semantic_map.get(uri.as_str())?;
    //         let rope = self.document_map.get(uri.as_str())?;
    //         let position = params.text_document_position_params.position;
    //         let offset = position_to_offset(position, &rope)?;

    //         let interval = semantic.ident_range.find(offset, offset + 1).next()?;
    //         let interval_val = interval.val;
    //         let range = match interval_val {
    //             IdentType::Binding(symbol_id) => {
    //                 let span = &semantic.table.symbol_id_to_span[symbol_id];
    //                 Some(span.clone())
    //             }
    //             IdentType::Reference(reference_id) => {
    //                 let reference = semantic.table.reference_id_to_reference.get(reference_id)?;
    //                 let symbol_id = reference.symbol_id?;
    //                 let symbol_range = semantic.table.symbol_id_to_span.get(symbol_id)?;
    //                 Some(symbol_range.clone())
    //             }
    //         };

    //         range.and_then(|range| {
    //             let start_position = offset_to_position(range.start, &rope)?;
    //             let end_position = offset_to_position(range.end, &rope)?;
    //             Some(GotoDefinitionResponse::Scalar(Location::new(
    //                 uri,
    //                 Range::new(start_position, end_position),
    //             )))
    //         })
    //     }();
    //     Ok(definition)
    // }

    // async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    //     let reference_list = || -> Option<Vec<Location>> {
    //         let uri = params.text_document_position.text_document.uri;
    //         let semantic = self.semantic_map.get(uri.as_str())?;
    //         let rope = self.document_map.get(uri.as_str())?;
    //         let position = params.text_document_position.position;
    //         let offset = position_to_offset(position, &rope)?;
    //         let reference_span_list = get_references(&semantic, offset, offset + 1, false)?;

    //         let ret = reference_span_list
    //             .into_iter()
    //             .filter_map(|range| {
    //                 let start_position = offset_to_position(range.start, &rope)?;
    //                 let end_position = offset_to_position(range.end, &rope)?;

    //                 let range = Range::new(start_position, end_position);

    //                 Some(Location::new(uri.clone(), range))
    //             })
    //             .collect::<Vec<_>>();
    //         Some(ret)
    //     }();
    //     Ok(reference_list)
    // }

    // async fn semantic_tokens_full(
    //     &self,
    //     params: SemanticTokensParams,
    // ) -> Result<Option<SemanticTokensResult>> {
    //     let uri = params.text_document.uri.to_string();
    //     debug!("semantic_token_full");
    //     let semantic_tokens = || -> Option<Vec<SemanticToken>> {
    //         let mut im_complete_tokens = self.semantic_token_map.get_mut(&uri)?;
    //         let rope = self.document_map.get(&uri)?;
    //         im_complete_tokens.sort_by(|a, b| a.start.cmp(&b.start));
    //         let mut pre_line = 0;
    //         let mut pre_start = 0;
    //         let semantic_tokens = im_complete_tokens
    //             .iter()
    //             .filter_map(|token| {
    //                 let line = rope.try_byte_to_line(token.start).ok()? as u32;
    //                 let first = rope.try_line_to_char(line as usize).ok()? as u32;
    //                 let start = rope.try_byte_to_char(token.start).ok()? as u32 - first;
    //                 let delta_line = line - pre_line;
    //                 let delta_start = if delta_line == 0 {
    //                     start - pre_start
    //                 } else {
    //                     start
    //                 };
    //                 let ret = Some(SemanticToken {
    //                     delta_line,
    //                     delta_start,
    //                     length: token.length as u32,
    //                     token_type: token.token_type as u32,
    //                     token_modifiers_bitset: 0,
    //                 });
    //                 pre_line = line;
    //                 pre_start = start;
    //                 ret
    //             })
    //             .collect::<Vec<_>>();
    //         Some(semantic_tokens)
    //     }();
    //     if let Some(semantic_token) = semantic_tokens {
    //         return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
    //             result_id: None,
    //             data: semantic_token,
    //         })));
    //     }
    //     Ok(None)
    // }

    // async fn semantic_tokens_range(
    //     &self,
    //     params: SemanticTokensRangeParams,
    // ) -> Result<Option<SemanticTokensRangeResult>> {
    //     let uri = params.text_document.uri.to_string();
    //     let semantic_tokens = || -> Option<Vec<SemanticToken>> {
    //         let im_complete_tokens = self.semantic_token_map.get(&uri)?;
    //         let rope = self.document_map.get(&uri)?;
    //         let mut pre_line = 0;
    //         let mut pre_start = 0;
    //         let semantic_tokens = im_complete_tokens
    //             .iter()
    //             .filter_map(|token| {
    //                 let line = rope.try_byte_to_line(token.start).ok()? as u32;
    //                 let first = rope.try_line_to_char(line as usize).ok()? as u32;
    //                 let start = rope.try_byte_to_char(token.start).ok()? as u32 - first;
    //                 let ret = Some(SemanticToken {
    //                     delta_line: line - pre_line,
    //                     delta_start: if start >= pre_start {
    //                         start - pre_start
    //                     } else {
    //                         start
    //                     },
    //                     length: token.length as u32,
    //                     token_type: token.token_type as u32,
    //                     token_modifiers_bitset: 0,
    //                 });
    //                 pre_line = line;
    //                 pre_start = start;
    //                 ret
    //             })
    //             .collect::<Vec<_>>();
    //         Some(semantic_tokens)
    //     }();
    //     Ok(semantic_tokens.map(|data| {
    //         SemanticTokensRangeResult::Tokens(SemanticTokens {
    //             result_id: None,
    //             data,
    //         })
    //     }))
    // }

    // async fn inlay_hint(
    //     &self,
    //     params: tower_lsp::lsp_types::InlayHintParams,
    // ) -> Result<Option<Vec<InlayHint>>> {
    //     debug!("inlay hint");
    //     let uri = &params.text_document.uri;
    //     let mut hashmap = HashMap::new();
    //     if let Some(ast) = self.ast_map.get(uri.as_str()) {
    //         ast.iter().for_each(|(func, _)| {
    //             type_inference(&func.body, &mut hashmap);
    //         });
    //     }

    //     let document = match self.document_map.get(uri.as_str()) {
    //         Some(rope) => rope,
    //         None => return Ok(None),
    //     };
    //     let inlay_hint_list = hashmap
    //         .into_iter()
    //         .map(|(k, v)| {
    //             (
    //                 k.start,
    //                 k.end,
    //                 match v {
    //                     nrs_language_server::nrs_lang::Value::Null => "null".to_string(),
    //                     nrs_language_server::nrs_lang::Value::Bool(_) => "bool".to_string(),
    //                     nrs_language_server::nrs_lang::Value::Num(_) => "number".to_string(),
    //                     nrs_language_server::nrs_lang::Value::Str(_) => "string".to_string(),
    //                 },
    //             )
    //         })
    //         .filter_map(|item| {
    //             // let start_position = offset_to_position(item.0, document)?;
    //             let end_position = offset_to_position(item.1, &document)?;
    //             let inlay_hint = InlayHint {
    //                 text_edits: None,
    //                 tooltip: None,
    //                 kind: Some(InlayHintKind::TYPE),
    //                 padding_left: None,
    //                 padding_right: None,
    //                 data: None,
    //                 position: end_position,
    //                 label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
    //                     value: item.2,
    //                     tooltip: None,
    //                     location: Some(Location {
    //                         uri: params.text_document.uri.clone(),
    //                         range: Range {
    //                             start: Position::new(0, 4),
    //                             end: Position::new(0, 10),
    //                         },
    //                     }),
    //                     command: None,
    //                 }]),
    //             };
    //             Some(inlay_hint)
    //         })
    //         .collect::<Vec<_>>();

    //     Ok(Some(inlay_hint_list))
    // }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let completions = || -> Result<Option<Vec<CompletionItem>>> {
            let completions = provide_completions(self, &position, &uri)?;

            let mut ret = Vec::with_capacity(completions.len());
            for item in completions {
                ret.push(item.into());
            }

            Ok(Some(ret))
        }()?;

        Ok(completions.map(CompletionResponse::Array))
    }

    // async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
    //     let workspace_edit = || -> Option<WorkspaceEdit> {
    //         let uri = params.text_document_position.text_document.uri;
    //         let semantic = self.semantic_map.get(uri.as_str())?;
    //         let rope = self.document_map.get(uri.as_str())?;
    //         let position = params.text_document_position.position;
    //         let offset = position_to_offset(position, &rope)?;
    //         let reference_list = get_references(&semantic, offset, offset + 1, true)?;

    //         let new_name = params.new_name;
    //         (!reference_list.is_empty()).then_some(()).map(|_| {
    //             let edit_list = reference_list
    //                 .into_iter()
    //                 .filter_map(|range| {
    //                     let start_position = offset_to_position(range.start, &rope)?;
    //                     let end_position = offset_to_position(range.end, &rope)?;
    //                     Some(TextEdit::new(
    //                         Range::new(start_position, end_position),
    //                         new_name.clone(),
    //                     ))
    //                 })
    //                 .collect::<Vec<_>>();
    //             let mut map = HashMap::new();
    //             map.insert(uri, edit_list);
    //             WorkspaceEdit::new(map)
    //         })
    //     }();
    //     Ok(workspace_edit)
    // }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        debug!("configuration changed!");
    }

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        debug!("workspace folders changed!");
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        debug!("watched files have changed!");
    }

    // async fn execute_command(&self, _: ExecuteCommandParams) -> Result<Option<Value>> {
    //     debug!("command executed!");

    //     match self.client.apply_edit(WorkspaceEdit::default()).await {
    //         Ok(res) if res.applied => self.client.log_message(MessageType::INFO, "applied").await,
    //         Ok(_) => self.client.log_message(MessageType::INFO, "rejected").await,
    //         Err(err) => self.client.log_message(MessageType::ERROR, err).await,
    //     }

    //     Ok(None)
    // }
}
#[derive(Debug, Deserialize, Serialize)]
struct InlayHintParams {
    path: String,
}

#[allow(unused)]
enum CustomNotification {}
impl Notification for CustomNotification {
    type Params = InlayHintParams;
    const METHOD: &'static str = "custom/notification";
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        analysis_map: Arc::new(DashMap::new()),
        ast_map: Arc::new(DashMap::new()),
        document_map: DashMap::new(),
        // semantic_token_map: DashMap::new(),
        // semantic_map: DashMap::new(),
        workspace_roots: RwLock::new(vec![]),
        nuxt_globals: DashMap::new(),
        nuxt_info: DashMap::new(),
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}

fn offset_to_position(offset: usize, rope: &Rope) -> Option<Position> {
    let line = rope.try_char_to_line(offset).ok()?;
    let first_char_of_line = rope.try_line_to_char(line).ok()?;
    let column = offset - first_char_of_line;
    Some(Position::new(line as u32, column as u32))
}

// fn position_to_offset(position: Position, rope: &Rope) -> Option<usize> {
//     let line_char_offset = rope.try_line_to_char(position.line as usize).ok()?;
//     let slice = rope.slice(0..line_char_offset + position.character as usize);
//     Some(slice.len_bytes())
// }

// fn get_references(
//     semantic: &Semantic,
//     start: usize,
//     end: usize,
//     include_definition: bool,
// ) -> Option<Vec<Span>> {
//     let interval = semantic.ident_range.find(start, end).next()?;
//     let interval_val = interval.val;
//     match interval_val {
//         IdentType::Binding(symbol_id) => {
//             let references = semantic.table.symbol_id_to_references.get(&symbol_id)?;
//             let mut reference_span_list: Vec<Span> = references
//                 .iter()
//                 .map(|reference_id| {
//                     semantic.table.reference_id_to_reference[*reference_id]
//                         .span
//                         .clone()
//                 })
//                 .collect();
//             if include_definition {
//                 let symbol_range = semantic.table.symbol_id_to_span.get(symbol_id)?;
//                 reference_span_list.push(symbol_range.clone());
//             }
//             Some(reference_span_list)
//         }
//         IdentType::Reference(_) => None,
//     }
// }
