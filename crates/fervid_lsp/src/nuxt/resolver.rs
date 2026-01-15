use std::path::{Path, PathBuf};

pub fn resolve_from_nuxt_dir(workspace_root: &Path, import_path: &str) -> PathBuf {
    // import_path is relative to .nuxt
    let nuxt_dir = workspace_root.join(".nuxt");
    nuxt_dir.join(import_path)
}
