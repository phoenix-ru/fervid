use swc_core::ecma::{
    ast::{Id, ImportDecl, ImportSpecifier, ModuleExportName},
    atoms::JsWord,
};

use crate::{
    atoms::{VUE, REF, COMPUTED, REACTIVE},
    structs::{ScriptLegacyVars, VueResolvedImports},
};

pub fn collect_imports(
    import_decl: &ImportDecl,
    out: &mut ScriptLegacyVars,
    vue_imports: &mut VueResolvedImports,
) {
    if import_decl.type_only {
        return;
    }

    for specifier in import_decl.specifiers.iter() {
        // examples below are from SWC
        match specifier {
            // e.g. `import * as foo from 'mod.js'`
            ImportSpecifier::Namespace(ns_spec) => out.imports.push(ns_spec.local.to_id()),

            // e.g. `import foo from 'mod.js'`
            ImportSpecifier::Default(default_spec) => out.imports.push(default_spec.local.to_id()),

            // e.g. `import { foo } from 'mod.js'` -> local = foo, imported = None
            // e.g. `import { foo as bar } from 'mod.js'` -> local = bar, imported = Some(foo)
            ImportSpecifier::Named(named_spec) => {
                if import_decl.src.value == *VUE {
                    if let Some(ref was_renamed_from) = named_spec.imported {
                        // Renamed: `import { ref as r }` or `import { "ref" as r }`
                        let imported_word = match was_renamed_from {
                            ModuleExportName::Ident(ident) => &ident.sym,
                            ModuleExportName::Str(s) => &s.value,
                        };

                        collect_vue_import(imported_word, named_spec.local.to_id(), vue_imports)
                    } else {
                        // Just `import { ref }`
                        collect_vue_import(
                            &named_spec.local.sym,
                            named_spec.local.to_id(),
                            vue_imports,
                        )
                    }
                } else {
                    out.imports.push(named_spec.local.to_id())
                }
            }
        }
    }
}

fn collect_vue_import(imported_word: &JsWord, used_as: Id, vue_imports: &mut VueResolvedImports) {
    if *imported_word == *REF {
        vue_imports.ref_import = Some(used_as)
    } else if *imported_word == *COMPUTED {
        vue_imports.computed = Some(used_as)
    } else if *imported_word == *REACTIVE {
        vue_imports.reactive = Some(used_as)
    }
}
