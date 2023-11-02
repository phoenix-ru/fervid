use fervid_core::{BindingTypes, SetupBinding};
use swc_core::ecma::ast::{ExportDecl, ExportSpecifier, ModuleExportName, NamedExport};

use crate::structs::VueResolvedImports;

use super::analyzer::analyze_decl;

/// Collects exports from e.g. `export { foo, bar as baz } from 'qux'`
pub fn collect_exports_named(
    named: &NamedExport,
    out: &mut Vec<SetupBinding>
) {
    if named.type_only {
        return;
    }

    // I am not too sure about the type of a named export binding,
    // as it is never documented or tested in the official compiler
    let binding_type = BindingTypes::SetupMaybeRef;

    for specifier in named.specifiers.iter() {
        match specifier {
            // export * as foo from 'src'
            ExportSpecifier::Namespace(ns_export) => {
                collect_module_export_name(&ns_export.name, out, binding_type)
            }

            // export foo from 'mod'
            // is this supposed to work?..
            ExportSpecifier::Default(export_default) => out.push(SetupBinding(
                export_default.exported.sym.to_owned(),
                binding_type,
            )),

            // export { foo } from 'mod'
            ExportSpecifier::Named(named_export_specifier) => {
                if named_export_specifier.is_type_only {
                    continue;
                }

                let exported = named_export_specifier
                    .exported
                    .as_ref()
                    .unwrap_or(&named_export_specifier.orig);

                collect_module_export_name(exported, out, binding_type)
            }
        }
    }
}

/// Collects an export from e.g. `export function foo() {}` or `export const bar = 'baz'`
pub fn collect_exports_decl(
    export_decl: &ExportDecl,
    out: &mut Vec<SetupBinding>,
    vue_imports: &mut VueResolvedImports,
) {
    analyze_decl(&export_decl.decl, out, vue_imports);
}

fn collect_module_export_name(
    module_export_name: &ModuleExportName,
    out: &mut Vec<SetupBinding>,
    binding_type: BindingTypes,
) {
    match module_export_name {
        ModuleExportName::Ident(ref ns_export_ident) => {
            out.push(SetupBinding(ns_export_ident.sym.to_owned(), binding_type))
        }

        ModuleExportName::Str(ref ns_export_str) => {
            out.push(SetupBinding(ns_export_str.value.to_owned(), binding_type))
        }
    }
}
