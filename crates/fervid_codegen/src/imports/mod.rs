use fervid_core::VueImports;
use swc_core::{
    common::DUMMY_SP,
    ecma::{
        ast::{
            Ident, ImportNamedSpecifier, ImportSpecifier,
            ModuleExportName,
        },
        atoms::JsWord,
    },
};

use super::context::CodegenContext;

impl CodegenContext {
    pub fn add_to_imports(&mut self, vue_import: VueImports) {
        self.bindings_helper.vue_imports |= vue_import;
    }

    pub fn get_and_add_import_str(&mut self, vue_import: VueImports) -> &'static str {
        self.add_to_imports(vue_import);
        vue_import.as_str()
    }

    pub fn get_and_add_import_ident(&mut self, vue_import: VueImports) -> JsWord {
        self.add_to_imports(vue_import);
        vue_import.as_atom()
    }

    /// Generates all the imports used by template generation.
    /// All of the imports come from 'vue'.
    pub fn generate_imports(&self) -> Vec<ImportSpecifier> {
        let mut result = Vec::new();
        for import in self.bindings_helper.vue_imports.into_iter() {
            let import_raw = import.as_str();

            let import_local = Ident {
                span: DUMMY_SP,
                sym: import.as_atom(),
                optional: false,
            };

            let import_vue = Some(ModuleExportName::Ident(Ident {
                span: DUMMY_SP,
                sym: JsWord::from(&import_raw[1..]),
                optional: false,
            }));

            result.push(ImportSpecifier::Named(ImportNamedSpecifier {
                span: DUMMY_SP,
                local: import_local,
                imported: import_vue,
                is_type_only: false,
            }));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::ecma::ast::{Str, ImportDecl};

    #[test]
    fn it_remembers_added_imports() {
        let mut ctx = CodegenContext::default();

        // Semi-random imports (one is duplicated)
        ctx.add_to_imports(VueImports::OpenBlock);
        ctx.add_to_imports(VueImports::CreateBlock);
        ctx.add_to_imports(VueImports::WithCtx);
        ctx.add_to_imports(VueImports::NormalizeClass);
        ctx.add_to_imports(VueImports::ToDisplayString);
        ctx.add_to_imports(VueImports::WithDirectives);
        ctx.add_to_imports(VueImports::WithModifiers);
        ctx.add_to_imports(VueImports::OpenBlock);

        assert_eq!(7, ctx.bindings_helper.vue_imports.into_iter().count());
    }

    #[test]
    fn it_generates_imports() {
        let mut ctx = CodegenContext::default();

        // Semi-random imports (one is duplicated)
        ctx.add_to_imports(VueImports::OpenBlock);
        ctx.add_to_imports(VueImports::CreateBlock);
        ctx.add_to_imports(VueImports::WithCtx);
        ctx.add_to_imports(VueImports::NormalizeClass);
        ctx.add_to_imports(VueImports::ToDisplayString);
        ctx.add_to_imports(VueImports::WithDirectives);
        ctx.add_to_imports(VueImports::WithModifiers);
        ctx.add_to_imports(VueImports::OpenBlock);

        let generated_imports = ctx.generate_imports();
        let vue_import_decl = ImportDecl {
            span: DUMMY_SP,
            specifiers: generated_imports,
            src: Box::new(Str {
                span: DUMMY_SP,
                value: "vue".into(),
                raw: None,
            }),
            type_only: false,
            with: None,
        };

        assert_eq!(crate::test_utils::to_str(vue_import_decl), "import{createBlock as _createBlock,normalizeClass as _normalizeClass,openBlock as _openBlock,toDisplayString as _toDisplayString,withCtx as _withCtx,withDirectives as _withDirectives,withModifiers as _withModifiers}from\"vue\";");
    }
}
