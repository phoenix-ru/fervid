use flagset::flags;
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

flags! {
    // #[derive(Clone, Copy)]
    pub enum VueImports: u64 {
        CreateBlock,
        CreateCommentVNode,
        CreateElementBlock,
        CreateElementVNode,
        CreateTextVNode,
        CreateVNode,
        Fragment,
        KeepAlive,
        NormalizeClass,
        NormalizeStyle,
        OpenBlock,
        RenderList,
        RenderSlot,
        ResolveComponent,
        ResolveDirective,
        ResolveDynamicComponent,
        Suspense,
        Teleport,
        ToDisplayString,
        Transition,
        TransitionGroup,
        VModelCheckbox,
        VModelRadio,
        VModelSelect,
        VModelText,
        VShow,
        WithCtx,
        WithDirectives,
        WithModifiers,
    }
}

impl CodegenContext {
    pub fn add_to_imports(&mut self, vue_import: VueImports) {
        self.used_imports |= vue_import;
    }

    pub fn get_import_str(vue_import: VueImports) -> &'static str {
        match vue_import {
            VueImports::CreateBlock => "_createBlock",
            VueImports::CreateCommentVNode => "_createCommentVNode",
            VueImports::CreateElementBlock => "_createElementBlock",
            VueImports::CreateElementVNode => "_createElementVNode",
            VueImports::CreateTextVNode => "_createTextVNode",
            VueImports::CreateVNode => "_createVNode",
            VueImports::Fragment => "_Fragment",
            VueImports::KeepAlive => "_KeepAlive",
            VueImports::NormalizeClass => "_normalizeClass",
            VueImports::NormalizeStyle => "_normalizeStyle",
            VueImports::OpenBlock => "_openBlock",
            VueImports::RenderList => "_renderList",
            VueImports::RenderSlot => "_renderSlot",
            VueImports::ResolveComponent => "_resolveComponent",
            VueImports::ResolveDirective => "_resolveDirective",
            VueImports::ResolveDynamicComponent => "_resolveDynamicComponent",
            VueImports::Suspense => "_Suspense",
            VueImports::Teleport => "_Teleport",
            VueImports::ToDisplayString => "_toDisplayString",
            VueImports::Transition => "_Transition",
            VueImports::TransitionGroup => "_TransitionGroup",
            VueImports::VModelCheckbox => "_vModelCheckbox",
            VueImports::VModelRadio => "_vModelRadio",
            VueImports::VModelSelect => "_vModelSelect",
            VueImports::VModelText => "_vModelText",
            VueImports::VShow => "_vShow",
            VueImports::WithCtx => "_withCtx",
            VueImports::WithDirectives => "_withDirectives",
            VueImports::WithModifiers => "_withModifiers",
        }
    }

    pub fn get_import_ident(vue_import: VueImports) -> JsWord {
        JsWord::from(Self::get_import_str(vue_import))
    }

    pub fn get_and_add_import_str(&mut self, vue_import: VueImports) -> &'static str {
        self.add_to_imports(vue_import);
        Self::get_import_str(vue_import)
    }

    pub fn get_and_add_import_ident(&mut self, vue_import: VueImports) -> JsWord {
        self.add_to_imports(vue_import);
        Self::get_import_ident(vue_import)
    }

    pub fn generate_imports_string(&self) -> String {
        let mut result = String::from("import { ");
        let mut has_first_import = false;

        for import in self.used_imports.into_iter() {
            if has_first_import {
                result.push_str(", ");
            }

            let import_str = Self::get_import_str(import);
            result.push_str(&import_str[1..]);
            result.push_str(" as ");
            result.push_str(import_str);

            has_first_import = true;
        }

        result.push_str(" } from \"vue\"");

        result
    }

    /// Generates all the imports used by template generation.
    /// All of the imports come from 'vue'.
    pub fn generate_imports(&self) -> Vec<ImportSpecifier> {
        let mut result = Vec::new();
        for import in self.used_imports.into_iter() {
            let import_raw = Self::get_import_str(import);

            let import_local = Ident {
                span: DUMMY_SP,
                sym: JsWord::from(import_raw),
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

        assert_eq!(7, ctx.used_imports.into_iter().count());
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
            asserts: None,
        };

        assert_eq!(crate::test_utils::to_str(vue_import_decl), "import{createBlock as _createBlock,normalizeClass as _normalizeClass,openBlock as _openBlock,toDisplayString as _toDisplayString,withCtx as _withCtx,withDirectives as _withDirectives,withModifiers as _withModifiers}from\"vue\";");
    }
}
