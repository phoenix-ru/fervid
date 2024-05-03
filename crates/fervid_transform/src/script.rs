//! Responsible for `<script>` and `<script setup>` transformations and analysis.

use fervid_core::{SfcScriptBlock, TemplateGenerationMode};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Function, Module, ObjectLit},
};

use crate::{error::TransformError, structs::TransformScriptsResult, BindingsHelper};

use self::{
    imports::process_imports,
    options_api::{transform_and_record_script_options_api, AnalyzeOptions},
    setup::{merge_sfc_helper, transform_and_record_script_setup},
};

pub mod common;
mod imports;
mod options_api;
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
    script_setup: Option<SfcScriptBlock>,
    mut script_legacy: Option<SfcScriptBlock>,
    bindings_helper: &mut BindingsHelper,
    errors: &mut Vec<TransformError>,
) -> TransformScriptsResult {
    // Set inline flag in `BindingsHelper`
    if bindings_helper.is_prod && script_setup.is_some() {
        bindings_helper.template_generation_mode = TemplateGenerationMode::Inline;
    }

    // 1.1. Imports in `<script>`
    if let Some(ref mut script_options) = script_legacy {
        process_imports(&mut script_options.content, bindings_helper, false, errors);
    }

    //
    // STEP 1: Transform Options API `<script>`.
    //

    let mut module: Module = script_legacy.map_or_else(
        || Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        },
        |script| *script.content,
    );

    let script_options_transform_result = transform_and_record_script_options_api(
        &mut module,
        AnalyzeOptions::default(),
        bindings_helper,
        errors,
    );

    //
    // STEP 2: Prepare the exported object.
    //

    let mut export_obj = script_options_transform_result
        .default_export_obj
        .unwrap_or_else(|| ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        });

    //
    // STEP 3: Transform the Composition API `<script setup>`.
    //

    let mut setup_fn: Option<Box<Function>> = None;
    if let Some(script_setup) = script_setup {
        let setup_transform_result =
            transform_and_record_script_setup(script_setup, bindings_helper, errors);

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
                ctxt: Default::default(),
            },
        };
        let script_setup = SfcScriptBlock {
            content: ts!(script_setup_content),
            lang: fervid_core::SfcScriptLang::Typescript,
            is_setup: true,
            span: Span {
                lo: swc_core::common::BytePos(script_content.len() as u32 + 2),
                hi: swc_core::common::BytePos(script_setup_content.len() as u32 + 1),
                ctxt: Default::default(),
            },
        };

        // Do work
        let mut bindings_helper = BindingsHelper::default();
        let mut errors = Vec::new();
        let res = transform_and_record_scripts(
            Some(script_setup),
            Some(script),
            &mut bindings_helper,
            &mut errors,
        );

        // Emitting the result requires some setup with SWC
        let cm: Lrc<SourceMap> = Default::default();
        let mut buff: Vec<u8> = Vec::with_capacity(128);
        let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

        // For possible errors, otherwise SWC does not like it
        let mut source = String::from(script_content);
        source.push_str(script_setup_content);
        cm.new_source_file(swc_core::common::FileName::Anon, source);

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
