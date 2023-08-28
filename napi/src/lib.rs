#![deny(clippy::all)]

#[cfg(not(all(target_os = "linux", target_env = "musl", target_arch = "aarch64")))]
#[global_allocator]
static ALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use fervid::parse_sfc;
use fervid_codegen::CodegenContext;
use fervid_transform::{
    script::transform_and_record_scripts, structs::ScopeHelper,
    template::transform_and_record_template,
};

#[napi]
pub fn compile_sync(source: String) -> Result<String> {
    let (_, sfc) = parse_sfc(&source).map_err(|err| {
        return Error::from_reason(err.to_string());
    })?;

    let mut template_block = sfc.template;
    let Some(ref mut template_block) = template_block else {
        panic!("This component has no template block");
    };

    let mut scope_helper = ScopeHelper::default();
    let module =
        transform_and_record_scripts(sfc.script_setup, sfc.script_legacy, &mut scope_helper);
    transform_and_record_template(template_block, &mut scope_helper);

    let mut ctx = CodegenContext::default();
    let template_expr = ctx.generate_sfc_template(&template_block);

    let sfc_module = ctx.generate_module(template_expr, module.0, module.1);

    let compiled_code = CodegenContext::stringify(&source, &sfc_module, false);

    Ok(compiled_code)
}
