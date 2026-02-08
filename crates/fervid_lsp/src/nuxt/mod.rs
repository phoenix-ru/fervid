use std::path::PathBuf;

use fervid_core::FervidAtom;

pub mod loader;
mod parser;
pub mod resolver;

#[derive(Debug, Default)]
pub struct NuxtInfo {
    /// Path to `.nuxt/imports.d.ts`
    #[allow(dead_code)]
    pub imports_dts: Option<PathBuf>,
    /// Path to `.nuxt/components.d.ts`
    #[allow(dead_code)]
    pub components_dts: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct NuxtGlobals {
    pub imports: fxhash::FxHashMap<FervidAtom, NuxtGlobalTarget>,
    pub components: fxhash::FxHashMap<FervidAtom, NuxtGlobalTarget>,
}

#[derive(Debug, Default)]
pub struct NuxtGlobalTarget {
    /// Raw specifier from .d.ts
    pub spec: FervidAtom,
    /// Resolved filesystem path
    pub resolved: Option<PathBuf>,
}

impl NuxtGlobalTarget {
    pub fn new(spec: FervidAtom) -> Self {
        NuxtGlobalTarget {
            spec,
            resolved: None,
        }
    }
}
