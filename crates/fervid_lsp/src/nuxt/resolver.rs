use crate::{
    nuxt::{NuxtGlobalTarget, NuxtGlobals},
    utils::normalize_path,
};
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;

const EXT_CANDIDATES: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "d.ts", "d.mts", "vue",
];

/// Resolve all globals' filesystem paths and cache them into `resolved`.
pub async fn resolve_global_paths(workspace_root: &Path, globals: &mut NuxtGlobals) {
    // Imports
    for (_name, target) in globals.imports.iter_mut() {
        resolve_one_target(workspace_root, target).await;
    }

    // Components
    for (_name, target) in globals.components.iter_mut() {
        resolve_one_target(workspace_root, target).await;
    }
}

async fn resolve_one_target(workspace_root: &Path, target: &mut NuxtGlobalTarget) {
    // Skip if already resolved
    if target.resolved.is_some() {
        return;
    }

    let spec_str = target.spec.as_ref();
    if is_bare_or_alias(spec_str) {
        // Can't resolve "vue-demi" etc here; keep unresolved.
        // TODO
        return;
    }

    let candidates = get_candidates_for_spec(workspace_root, spec_str);

    if let Some(found) = find_best_existing_path(candidates).await {
        target.resolved = Some(normalize_path(found));
    }
}

fn module_candidates_from_nuxt_dir(workspace_root: &Path, import_path: &str) -> Vec<PathBuf> {
    // import_path is relative to .nuxt
    let nuxt_dir = workspace_root.join(".nuxt");
    module_candidates_from_base(&nuxt_dir, import_path)
}

/// Build candidates for a spec, interpreting it relative to `.nuxt/` when appropriate.
/// If spec already has an extension, candidates include the direct path.
fn get_candidates_for_spec(workspace_root: &Path, spec: &str) -> Vec<PathBuf> {
    // Absolute paths: just try as-is (+ optional index forms if no ext)
    if spec.starts_with('/') || spec.starts_with(r"\\") || spec.contains(':') {
        return module_candidates_from_base(Path::new(""), spec);
    }

    // Relative to .nuxt
    module_candidates_from_nuxt_dir(workspace_root, spec)
}

/// Same candidate logic as before, but with an arbitrary base.
/// Used to support absolute paths (base == "").
fn module_candidates_from_base(base: &Path, spec: &str) -> Vec<PathBuf> {
    let spec_path = Path::new(spec);
    let mut out = Vec::new();

    // Use direct path if file extension is present
    if spec_path.extension().is_some() {
        out.push(base.join(spec));
        return out;
    }

    // 1) <spec>.<ext>
    for ext in EXT_CANDIDATES {
        out.push(base.join(format!("{spec}.{ext}")));
    }

    // 2) <spec>/index.<ext>
    let sub_dir = base.join(spec);
    for ext in EXT_CANDIDATES {
        out.push(sub_dir.join(format!("index.{ext}")));
    }

    out
}

async fn find_best_existing_path(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    let mut set = JoinSet::new();

    for (priority, candidate) in candidates.into_iter().enumerate() {
        set.spawn(try_exists_with_priority(candidate, priority));
    }

    let results = set.join_all().await;
    let mut best_candidate = None;
    for result in results {
        let Ok(result) = result else {
            continue;
        };

        match best_candidate {
            None => best_candidate = Some(result),
            // Lower number = higher priority
            Some(existing_best) if result.1 < existing_best.1 => best_candidate = Some(result),
            _ => {}
        }
    }

    best_candidate.map(|v| v.0)
}

async fn try_exists_with_priority(
    path: PathBuf,
    priority: usize,
) -> Result<(PathBuf, usize), std::io::Error> {
    match tokio::fs::try_exists(&path).await {
        Ok(true) => Ok((path, priority)),
        Ok(false) => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "")),
        Err(e) => Err(e),
    }
}

/// Decide whether a spec looks like a filesystem-like path we can resolve
fn is_path_like(spec: &str) -> bool {
    spec.starts_with("./")
        || spec.starts_with("../")
        || spec.starts_with('/') // unix absolute
        || spec.starts_with(r"\\") // windows UNC
        || spec.contains(':') // windows drive like C:\...
}

// TODO: Implement it
/// Bare specifiers like "vue-demi", "@vueuse/core", "#app/..." are NOT resolved here.
fn is_bare_or_alias(spec: &str) -> bool {
    // "foo", "@scope/foo", "#app/...", "~", "@", etc
    // For now, treat anything not path-like as non-resolvable
    !is_path_like(spec)
}
