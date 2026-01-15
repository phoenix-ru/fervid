use std::path::PathBuf;

pub mod loader;
mod parser;
pub mod resolver;

#[derive(Debug, Default)]
pub struct NuxtInfo {
    /// Path to `.nuxt/imports.d.ts`
    pub imports_dts: Option<PathBuf>,
    /// Path to `.nuxt/components.d.ts`
    pub components_dts: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct NuxtGlobals {
    pub imports: fxhash::FxHashMap<String, String>,
    pub components: fxhash::FxHashMap<String, String>,
}
