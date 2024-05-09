#![deny(clippy::all)]

use std::fs;

use farmfe_core::{
    config::Config,
    error::CompilationError,
    module::ModuleType,
    parking_lot::Mutex,
    plugin::{Plugin, PluginLoadHookResult, PluginTransformHookResult},
};
use fervid::CompileOptions;
use fxhash::FxHashMap;

use farmfe_macro_plugin::farm_plugin;

#[farm_plugin]
pub struct FarmPluginVueFervid {
    virtual_modules: Mutex<FxHashMap<String, PluginLoadHookResult>>,
}

impl FarmPluginVueFervid {
    fn new(_config: &Config, _options: String) -> Self {
        Self {
            virtual_modules: Default::default(),
        }
    }
}

impl Plugin for FarmPluginVueFervid {
    fn name(&self) -> &str {
        "FarmPluginVueFervid"
    }

    fn load(
        &self,
        param: &farmfe_core::plugin::PluginLoadHookParam,
        _context: &std::sync::Arc<farmfe_core::context::CompilationContext>,
        _hook_context: &farmfe_core::plugin::PluginHookContext,
    ) -> farmfe_core::error::Result<Option<farmfe_core::plugin::PluginLoadHookResult>> {
        // println!(
        //     "load path: {:?}, id: {:?}",
        //     param.resolved_path, param.module_id
        // );

        // Virtual modules
        if param.query.iter().any(|it| it.0 == "vue") {
            let virtual_modules = self.virtual_modules.lock();
            let Some(load_result) = virtual_modules.get(&param.module_id) else {
                return Ok(None);
            };

            // We have to re-create because this struct is not `Clone`
            return Ok(Some(PluginLoadHookResult {
                content: load_result.content.to_owned(),
                module_type: load_result.module_type.to_owned(),
                source_map: load_result.source_map.to_owned(),
            }));
        }

        if param.resolved_path.ends_with(".vue") {
            let content = fs::read_to_string(param.resolved_path).expect("Should exist");

            return Ok(Some(PluginLoadHookResult {
                content,
                module_type: ModuleType::Custom("vue".to_string()),
                source_map: None,
            }));
        }

        Ok(None)
    }

    fn transform(
        &self,
        param: &farmfe_core::plugin::PluginTransformHookParam,
        _context: &std::sync::Arc<farmfe_core::context::CompilationContext>,
    ) -> farmfe_core::error::Result<Option<farmfe_core::plugin::PluginTransformHookResult>> {
        // Guard
        if !matches!(param.module_type, ModuleType::Custom(ref typ) if typ == "vue") {
            return Ok(None);
        }

        let file_compile_result = fervid::compile(
            &param.content,
            CompileOptions {
                filename: std::borrow::Cow::Borrowed(param.resolved_path),
                id: param.module_id.clone().into(),
                is_prod: Some(true),
                ssr: None,
                gen_default_as: None,
                source_map: None
            },
        );

        let Ok(compile_result) = file_compile_result else {
            return Err(CompilationError::TransformError {
                resolved_path: param.resolved_path.to_owned(),
                msg: "Failed to compile".into(),
            });
        };

        let mut prepend = String::new();
        if !compile_result.styles.is_empty() {
            let mut virtual_modules = self.virtual_modules.lock();

            let base_path = param.resolved_path;
            let module_id = &param.module_id;

            for (idx, style) in compile_result.styles.into_iter().enumerate() {
                let lang = style.lang;
                let query = format!("?vue&type=style&idx={idx}&lang={lang}");
                let virtual_module_id = format!("{module_id}{query}");
                prepend.push_str(&format!("import '{base_path}{query}'\n"));

                virtual_modules.insert(virtual_module_id, PluginLoadHookResult {
                    content: style.code,
                    // TODO Determine based on style lang
                    module_type: ModuleType::Css,
                    source_map: None,
                });
            }
        }

        let mut content = compile_result.code;
        if !prepend.is_empty() {
            content.insert_str(0, &prepend);
        }

        return Ok(Some(PluginTransformHookResult {
            content,
            module_type: Some(ModuleType::Ts),
            source_map: None,
            ignore_previous_source_map: false,
        }));
    }
}
