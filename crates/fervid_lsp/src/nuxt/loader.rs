use std::sync::Arc;

use tokio::fs;
use tower_lsp::lsp_types::{MessageType, WorkspaceFolder};

use crate::nuxt::parser::{parse_globals, ParseGlobalsResult};
use crate::nuxt::NuxtInfo;
use crate::utils::workspace_uri_to_path;
use crate::Backend;

pub async fn load_nuxt_for_workspaces(backend: &Backend, workspace_folders: &[WorkspaceFolder]) {
    for workspace_folder in workspace_folders {
        let Some(root) = workspace_uri_to_path(workspace_folder) else {
            continue;
        };
        let root_key = root.to_string_lossy().to_string();

        // Find .nuxt dir
        let nuxt_dir = root.join(".nuxt");
        if !nuxt_dir.is_dir() {
            continue;
        }

        let imports_path = nuxt_dir.join("imports.d.ts");
        let components_path = nuxt_dir.join("components.d.ts");

        // Read files async, ignore missing
        let imports_source = match fs::read_to_string(&imports_path).await {
            Ok(s) => Some(s),
            Err(_) => None,
        };
        let components_source = match fs::read_to_string(&components_path).await {
            Ok(s) => Some(s),
            Err(_) => None,
        };

        let ParseGlobalsResult {
            globals,
            is_error_loading_components,
            is_error_loading_imports,
        } = parse_globals(
            imports_source.as_deref(),
            &imports_path,
            components_source.as_deref(),
            &components_path,
        );

        // Store resolved paths even if missing
        backend.nuxt_info.insert(
            root_key.clone(),
            Arc::new(NuxtInfo {
                imports_dts: imports_path.exists().then_some(imports_path.clone()),
                components_dts: components_path.exists().then_some(components_path.clone()),
            }),
        );

        backend
            .nuxt_globals
            .insert(root_key.clone(), Arc::new(globals));

        // Log warnings
        if is_error_loading_imports {
            let _ = backend
                .client
                .log_message(
                    MessageType::WARNING,
                    format!("Could not load imports.d.ts for {}", root_key),
                )
                .await;
        }
        if is_error_loading_components {
            let _ = backend
                .client
                .log_message(
                    MessageType::WARNING,
                    format!("Could not load components.d.ts for {}", root_key),
                )
                .await;
        }

        let _ = backend
            .client
            .log_message(
                MessageType::INFO,
                format!("Loaded Nuxt globals for {}", root_key),
            )
            .await;
    }
}
