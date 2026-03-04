use std::{path::PathBuf, sync::Arc};

use fervid_core::FervidAtom;
use fxhash::FxHashMap;
use swc_core::{
    common::{FileName, SourceMap},
    ecma::ast::{
        Decl, ExportNamedSpecifier, ExportSpecifier, Module, ModuleDecl, ModuleExportName,
        ModuleItem, Pat, TsType, TsTypeElement, TsTypeQuery, TsTypeQueryExpr,
        TsUnionOrIntersectionType,
    },
};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

use crate::nuxt::{NuxtGlobalTarget, NuxtGlobals};

pub struct ParseGlobalsResult {
    pub globals: NuxtGlobals,
    pub is_error_loading_imports: bool,
    pub is_error_loading_components: bool,
}

pub fn parse_globals(
    imports_source: Option<&str>,
    imports_path: &PathBuf,
    components_source: Option<&str>,
    components_path: &PathBuf,
) -> ParseGlobalsResult {
    // TODO Research a way to share the source map across threads from Backend?
    let source_map = Arc::new(SourceMap::default());

    // Parse the Nuxt files for globals
    let mut errors = Vec::new();
    let mut is_error_loading_imports = false;
    let mut is_error_loading_components = false;

    let imports = imports_source
        .and_then(
            |src| match parse_nuxt_imports_dts(src, imports_path, source_map.clone()) {
                Ok(v) => Some(v),
                Err(error) => {
                    errors.push(error);
                    is_error_loading_imports = true;
                    None
                }
            },
        )
        .unwrap_or_default();

    let components = components_source
        .and_then(
            |src| match parse_nuxt_components_dts(src, components_path, source_map.clone()) {
                Ok(v) => Some(v),
                Err(error) => {
                    errors.push(error);
                    is_error_loading_components = true;
                    None
                }
            },
        )
        .unwrap_or_default();

    ParseGlobalsResult {
        globals: NuxtGlobals {
            imports,
            components,
        },
        is_error_loading_components,
        is_error_loading_imports,
    }
}

fn parse_nuxt_components_dts(
    source: &str,
    path: &PathBuf,
    cm: Arc<SourceMap>,
) -> Result<FxHashMap<FervidAtom, NuxtGlobalTarget>, swc_ecma_parser::error::Error> {
    let module = parse_ts_module(source, path, &cm)?;
    let mut out: FxHashMap<FervidAtom, NuxtGlobalTarget> = FxHashMap::default();

    for item in module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) = item else {
            continue;
        };

        let Decl::Var(var_decl) = &export_decl.decl else {
            continue;
        };

        for decl in &var_decl.decls {
            // export const X: <type>
            let Pat::Ident(ident) = &decl.name else {
                continue;
            };
            let Some(type_ann) = &ident.type_ann else {
                continue;
            };

            if let Some(import_path) = find_first_import_type_path(&type_ann.type_ann) {
                out.insert(ident.id.sym.to_owned(), NuxtGlobalTarget::new(import_path));
            }
        }
    }

    Ok(out)
}

fn parse_nuxt_imports_dts(
    source: &str,
    path: &PathBuf,
    cm: Arc<SourceMap>,
) -> Result<FxHashMap<FervidAtom, NuxtGlobalTarget>, swc_ecma_parser::error::Error> {
    let module = parse_ts_module(source, path, &cm)?;
    let mut out: FxHashMap<FervidAtom, NuxtGlobalTarget> = FxHashMap::default();

    for item in module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named)) = item else {
            continue;
        };

        // We only care about `export { ... } from "..."` (re-export)
        let Some(src) = &named.src else {
            continue;
        };

        let specifier = src.value.to_owned();

        for s in &named.specifiers {
            match s {
                ExportSpecifier::Named(ExportNamedSpecifier { exported, orig, .. }) => {
                    // exported name (alias) if present, otherwise orig
                    let name = exported
                        .as_ref()
                        .map(module_export_name_to_string)
                        .unwrap_or_else(|| module_export_name_to_string(orig));

                    out.insert(name, NuxtGlobalTarget::new(specifier.to_owned()));
                }
                ExportSpecifier::Default(_) => {
                    // `export { default as X } from ...` comes as Named with orig=Ident("default")
                }
                ExportSpecifier::Namespace(ns) => {
                    // `export * as foo from "..."` -> foo
                    let name = module_export_name_to_string(&ns.name);
                    out.insert(name, NuxtGlobalTarget::new(specifier.to_owned()));
                }
            }
        }
    }

    Ok(out)
}

/// Walk TS types to find `import("...")` path.
fn find_first_import_type_path(ty: &TsType) -> Option<FervidAtom> {
    match ty {
        TsType::TsImportType(import_ty) => Some(import_ty.arg.value.to_owned()),

        TsType::TsTypeQuery(q) => {
            // typeof import("...").default often becomes a TsTypeQuery whose expr_name
            // contains TsImportType, depending on parsing form. So recursively inspect children.
            find_import_in_type_query(q)
        }

        TsType::TsTypeRef(r) => {
            // Unlikely here, but keep it recursive
            r.type_params
                .as_ref()?
                .params
                .iter()
                .find_map(|arg0| find_first_import_type_path(arg0))
        }

        TsType::TsTypeLit(lit) => {
            for m in &lit.members {
                if let TsTypeElement::TsPropertySignature(p) = m
                    && let Some(t) = &p.type_ann
                    && let Some(found) = find_first_import_type_path(&t.type_ann)
                {
                    return Some(found);
                }
            }
            None
        }

        TsType::TsUnionOrIntersectionType(u) => match u {
            TsUnionOrIntersectionType::TsUnionType(u) => u
                .types
                .iter()
                .find_map(|arg0| find_first_import_type_path(arg0)),
            TsUnionOrIntersectionType::TsIntersectionType(i) => i
                .types
                .iter()
                .find_map(|arg0| find_first_import_type_path(arg0)),
        },

        TsType::TsParenthesizedType(p) => find_first_import_type_path(&p.type_ann),

        TsType::TsIndexedAccessType(i) => find_first_import_type_path(&i.obj_type)
            .or_else(|| find_first_import_type_path(&i.index_type)),

        TsType::TsTypeOperator(op) => find_first_import_type_path(&op.type_ann),

        TsType::TsConditionalType(c) => find_first_import_type_path(&c.check_type)
            .or_else(|| find_first_import_type_path(&c.extends_type))
            .or_else(|| find_first_import_type_path(&c.true_type))
            .or_else(|| find_first_import_type_path(&c.false_type)),

        _ => None,
    }
}

fn find_import_in_type_query(q: &TsTypeQuery) -> Option<FervidAtom> {
    match &q.expr_name {
        TsTypeQueryExpr::Import(import_ty) => Some(import_ty.arg.value.to_owned()),
        TsTypeQueryExpr::TsEntityName(_) => None,
    }
}

fn module_export_name_to_string(n: &ModuleExportName) -> FervidAtom {
    match n {
        ModuleExportName::Ident(i) => i.sym.to_owned(),
        ModuleExportName::Str(s) => s.value.to_owned(),
    }
}

fn parse_ts_module(
    source: &str,
    path: &PathBuf,
    cm: &SourceMap,
) -> Result<Module, swc_ecma_parser::error::Error> {
    let filename = FileName::Real(path.to_owned());
    let fm = cm.new_source_file(filename.into(), source.to_string());

    let lexer = Lexer::new(
        Syntax::Typescript(TsSyntax::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    let module = parser.parse_module()?;

    // TODO: collect parser.take_errors() and log them
    Ok(module)
}
