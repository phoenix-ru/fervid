use crate::SetupBinding;
use fervid_core::BindingTypes;
use swc_core::ecma::ast::{ExportSpecifier, ModuleExportName, NamedExport};

/// Collects exports from e.g. `export { foo, bar as baz } from 'qux'`
pub fn collect_exports_named(named: &NamedExport, out: &mut Vec<SetupBinding>) {
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
            ExportSpecifier::Default(export_default) => out.push(SetupBinding::new_spanned(
                export_default.exported.sym.to_owned(),
                binding_type,
                export_default.exported.span,
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

fn collect_module_export_name(
    module_export_name: &ModuleExportName,
    out: &mut Vec<SetupBinding>,
    binding_type: BindingTypes,
) {
    match module_export_name {
        ModuleExportName::Ident(ref ns_export_ident) => out.push(SetupBinding::new_spanned(
            ns_export_ident.sym.to_owned(),
            binding_type,
            ns_export_ident.span,
        )),

        ModuleExportName::Str(ref ns_export_str) => out.push(SetupBinding::new_spanned(
            ns_export_str.value.to_owned(),
            binding_type,
            ns_export_str.span,
        )),
    }
}
