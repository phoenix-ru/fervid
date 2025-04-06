//! Responsible for `<script>` and `<script setup>` transformations and analysis.

use fervid_core::SfcScriptBlock;
use resolve_type::record_types;
use setup::define_props_destructure::collect_props_destructure;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Callee, Decl, Expr, Function, Module, ModuleItem, ObjectLit, Pat, Stmt},
};

use crate::{
    atoms::{DEFINE_EMITS, DEFINE_PROPS},
    error::TransformError,
    structs::TransformScriptsResult,
    TransformSfcContext,
};

use self::{
    imports::process_imports,
    options_api::{transform_and_record_script_options_api, AnalyzeOptions},
    setup::{merge_sfc_helper, transform_and_record_script_setup},
};

pub mod common;
mod imports;
mod options_api;
mod resolve_type;
mod setup;
pub mod utils;

/// Transforms two script modules: `<script>` and `<script setup>`.
/// Returns a combined Module and a default export object.
///
/// Consumes both [`SfcScriptBlock`]s to avoid cloning.
///
/// It will populate the provided [`BindingsHelper`] with the analysis information, such as:
/// - Variable bindings (from `<script setup>` and from Options API);
/// - Import bindings;
/// - (TODO) Imported `.vue` component bindings;
pub fn transform_and_record_scripts(
    ctx: &mut TransformSfcContext,
    mut script_setup: Option<SfcScriptBlock>,
    mut script_options: Option<SfcScriptBlock>,
    errors: &mut Vec<TransformError>,
) -> TransformScriptsResult {
    //
    // STEP 1: Imports and type collection.
    //
    // Imports are collected early because ES6 imports are hoisted and usage like this is valid:
    // ```ts
    // const bar = x(1)
    // import { reactive as x } from 'vue'
    // ```
    //
    // Official compiler does lazy type recording using the source AST,
    // but we are modifying the source AST and thus cannot use it at a later stage.
    // Therefore, types are eagerly recorded.
    pretransform(ctx, script_setup.as_mut(), script_options.as_mut(), errors);

    //
    // STEP 2: Transform Options API `<script>`.
    //
    let mut script_module: Option<Box<Module>> = None;
    let mut script_default_export: Option<ObjectLit> = None;

    if let Some(script_options_block) = script_options {
        let mut module = script_options_block.content;

        let transform_result = transform_and_record_script_options_api(
            &mut module,
            AnalyzeOptions {
                collect_top_level_stmts: script_setup.is_some(),
                ..Default::default()
            },
            &mut ctx.bindings_helper,
            errors,
        );

        script_module = Some(module);
        script_default_export = transform_result.default_export_obj;
    }

    //
    // STEP 2: Prepare the exported object and module
    //

    let mut module = script_module.unwrap_or_else(|| {
        Box::new(Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        })
    });
    let mut export_obj = script_default_export.unwrap_or_else(|| ObjectLit {
        span: DUMMY_SP,
        props: vec![],
    });

    //
    // STEP 3: Transform the Composition API `<script setup>`.
    //

    let mut setup_fn: Option<Box<Function>> = None;
    if let Some(script_setup) = script_setup {
        let setup_transform_result = transform_and_record_script_setup(ctx, script_setup, errors);

        // TODO Push imports at module top or bottom? Or smart merge?
        // TODO Merge Vue imports produced by module transformation
        for module_item in setup_transform_result.module_items.into_iter() {
            module.body.push(module_item);
        }

        // Merge fields into an SFC exported object
        merge_sfc_helper(
            setup_transform_result.sfc_object_helper,
            &mut export_obj.props,
        );

        // TODO Adding bindings to `setup()` in Options API will get overwritten in `<script setup>`
        // https://play.vuejs.org/#eNp9U01v2zAM/SuELm6BNFmTm5F22IYetsM2bMUudTEYNp2okyVDklMPQf77SNpunS7txTQfH/n4Ye/Vh6aZ71pUqVpHrBuTR7zOLAB5IV4Urm7EFaAPw+5CV1eZir7FTA1RgMq5gbg4KnScGYyLKVGf0rb6ZBa7z/pDQ//rB2qA7cvs7ZJYaAL21CqnV6KKXS+2y4G1GljX/CB8NWqVekehynlK/g3awipTBBRtiK7mMbbucVJ3vaCEMZdHBJvXSAQ2pRAYPTFJL3F2pwm7nAGb5T1ZW2J3zsJGh0gF9nuJXcLhcDQr16OYa6J2NlB0kNC2aSPVr12JhhTE/soNnwzS+Lfh7qR9eA9JxC4mkEJSUtVERp3ujetg7Qi4o9PdC+BswfovmlmHwusmQsDY8uF03TgfgW/5iU4Jlaf1JXM5Ln92CScV1HmE25FzBQnBtDEpNS1L79hJwRKrvDUR9jysiJ2d9w6AJ9fb0YNxNynIBysgbUkesq1ePifddxNZNVMxUKjSm/lDcJZ+EKmYKf4mtUH/ra+bqXTUylRujHv8IhirzUa82GLx5wT+EDrGMvXdY0C/o2U/xWLuN0i35/DNz690okmQ7tkaYr8R/IHBmZZ77GkfW1tS2xOedPtZTqTt5jbcdBFtGIca13UQfqboXHyf10Z/bnc1X437VYd/HFh0XQ==
        setup_fn = setup_transform_result.setup_fn;
    }

    TransformScriptsResult {
        module,
        export_obj,
        setup_fn,
    }
}

/// Records the imports and collects information for macro transforms,
/// such as types, variables for destructure, etc.
///
/// Phases:
/// 1. Import analysis and de-duplication;
/// 2. Collecting the information about used macros;
/// 3. If type-only macros found in phase 2, record the types from both the `<script>`s
fn pretransform(
    ctx: &mut TransformSfcContext,
    mut script_setup: Option<&mut SfcScriptBlock>,
    mut script_options: Option<&mut SfcScriptBlock>,
    errors: &mut Vec<TransformError>,
) {
    // Imports in `<script>`
    if let Some(ref mut script_options) = script_options {
        process_imports(
            &mut script_options.content,
            &mut ctx.bindings_helper,
            false,
            errors,
        );
    }

    let mut has_type_only_macros = false;
    let has_define_props_destructure = false;

    // Imports in `<script setup>`
    if let Some(ref mut script_setup) = script_setup {
        process_imports(
            &mut script_setup.content,
            &mut ctx.bindings_helper,
            true,
            errors,
        );

        // Walk the `<script setup>` to find macro usages
        // TODO Think of a way to apply multiple different AST transformations
        // Can it be done

        // TODO
        // 1. Walk `<script setup>` and find `defineProps` or `defineEmits`
        let has_define_props_or_emits =
            find_define_props_or_emits(ctx, &script_setup.content, errors);

        // 2. For `defineProps`, check if it is not an identifier, record bound variables
        // 3. Walk `<script setup>` and replace ident usages (deep)
        // 4. Record types if there is a need

        has_type_only_macros = has_define_props_or_emits.has_type_only_define_props_or_emits;
    }

    // 1.3. Record types to support type-only `defineProps` and `defineEmits`
    if has_type_only_macros {
        let scope = ctx.root_scope();
        let mut scope = (*scope).borrow_mut();
        scope.imports = ctx.bindings_helper.user_imports.clone();

        record_types(ctx, script_setup, script_options, &mut scope, false);
    }
}

struct DefinePropsOrEmitsMeta {
    has_type_only_define_props_or_emits: bool,
}

fn find_define_props_or_emits(
    ctx: &mut TransformSfcContext,
    module: &Module,
    errors: &mut Vec<TransformError>,
) -> DefinePropsOrEmitsMeta {
    let mut result = DefinePropsOrEmitsMeta {
        has_type_only_define_props_or_emits: false,
    };

    for module_item in module.body.iter() {
        let ModuleItem::Stmt(module_stmt) = module_item else {
            continue;
        };

        match module_stmt {
            // E.g. `let foo = defineProps()`
            Stmt::Decl(Decl::Var(var_decl)) => {
                if var_decl.declare {
                    continue;
                }

                for var_decl_item in var_decl.decls.iter() {
                    let Some(ref init) = var_decl_item.init else {
                        continue;
                    };

                    let Expr::Call(ref call_expr) = init.as_ref() else {
                        continue;
                    };

                    let Callee::Expr(ref callee_expr) = call_expr.callee else {
                        continue;
                    };

                    let Expr::Ident(ref callee_ident) = callee_expr.as_ref() else {
                        continue;
                    };

                    let is_define_props = callee_ident.sym != *DEFINE_PROPS;
                    let is_define_emits = callee_ident.sym != *DEFINE_EMITS;
                    if !is_define_props && !is_define_emits {
                        continue;
                    }

                    // Type-only marker
                    if call_expr.type_args.is_some() {
                        result.has_type_only_define_props_or_emits = true;
                    }

                    // No other logic for `defineEmits`
                    if is_define_emits {
                        continue;
                    }

                    // Props destructure supports only object patterns
                    if let Pat::Object(ref obj_pat) = var_decl_item.name {
                        collect_props_destructure(ctx, obj_pat, errors)
                    }
                }
            }

            Stmt::Expr(expr_stmt) => {
                let Expr::Call(ref call_expr) = expr_stmt.expr.as_ref() else {
                    continue;
                };

                let Callee::Expr(ref callee_expr) = call_expr.callee else {
                    continue;
                };

                let Expr::Ident(ref callee_ident) = callee_expr.as_ref() else {
                    continue;
                };

                let is_define_props = callee_ident.sym != *DEFINE_PROPS;
                let is_define_emits = callee_ident.sym != *DEFINE_EMITS;
                if !is_define_props && !is_define_emits {
                    continue;
                }

                // Type-only marker
                if call_expr.type_args.is_some() {
                    result.has_type_only_define_props_or_emits = true;
                }
            }

            _ => continue,
        };

        // Break if sufficient information is collected
        // if result.define_props_destructured_vars.len() > 0
        //     && result.has_type_only_define_props_or_emits
        // {
        //     break;
        // }
    }

    result
}

#[cfg(test)]
mod tests {
    use swc_core::common::{sync::Lrc, SourceMap, Span};
    use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};

    use super::*;
    use crate::test_utils::parser::parse_javascript_module;

    /// https://github.com/vuejs/core/blob/c0c9432b64091fa15fd8619cfb06828735356a42/packages/compiler-sfc/__tests__/compileScript.spec.ts#L261-L275
    #[test]
    fn import_dedupe_between_script_and_script_setup() {
        check_import_dedupe(
            "import { x } from './x'",
            "
            import { x } from './x'
            x()",
            "import { x } from './x';\n",
        )
    }

    #[test]
    fn it_deduplicates_imports() {
        check_import_dedupe(
            "
            import { x } from './x'
            import { ref } from 'vue'",
            "
            import { x, y, z } from './x'
            x()
            const foo = ref()",
            "import { x } from './x';\nimport { ref } from 'vue';\nimport { y, z } from './x';\n",
        );
    }

    fn check_import_dedupe(script_content: &str, script_setup_content: &str, expected: &str) {
        macro_rules! ts {
            ($input: expr) => {
                Box::new(
                    parse_javascript_module($input, 0, Default::default())
                        .expect("analyze_ts expects the input to be parseable")
                        .0,
                )
            };
        }

        let script = SfcScriptBlock {
            content: ts!(script_content),
            lang: fervid_core::SfcScriptLang::Typescript,
            is_setup: false,
            span: Span {
                lo: swc_core::common::BytePos(1),
                hi: swc_core::common::BytePos(script_content.len() as u32 + 1),
            },
        };
        let script_setup = SfcScriptBlock {
            content: ts!(script_setup_content),
            lang: fervid_core::SfcScriptLang::Typescript,
            is_setup: true,
            span: Span {
                lo: swc_core::common::BytePos(script_content.len() as u32 + 2),
                hi: swc_core::common::BytePos(script_setup_content.len() as u32 + 1),
            },
        };

        // Context for testing
        let mut ctx = TransformSfcContext::anonymous();

        // Do work
        let mut errors = Vec::new();
        let res =
            transform_and_record_scripts(&mut ctx, Some(script_setup), Some(script), &mut errors);

        // Emitting the result requires some setup with SWC
        let cm: Lrc<SourceMap> = Default::default();
        let mut buff: Vec<u8> = Vec::with_capacity(128);
        let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

        // For possible errors, otherwise SWC does not like it
        let mut source = String::from(script_content);
        source.push_str(script_setup_content);
        cm.new_source_file(Lrc::new(swc_core::common::FileName::Anon), source);

        let mut emitter_cfg = swc_ecma_codegen::Config::default();
        emitter_cfg.minify = false;
        emitter_cfg.omit_last_semi = false;

        let mut emitter = Emitter {
            cfg: emitter_cfg,
            comments: None,
            wr: writer,
            cm,
        };

        let _ = res.module.emit_with(&mut emitter);

        let stringified = String::from_utf8(buff).unwrap();

        assert_eq!(expected, stringified);
    }
}
