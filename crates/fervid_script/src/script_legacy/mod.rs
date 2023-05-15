use swc_core::ecma::{ast::Module, atoms::JsWord};

use self::utils::{find_default_export, find_setup_function, find_data_function, collect_fn_return_fields};

mod utils;

#[derive(Debug, Default)]
struct ScriptLegacyVars {
    // TODO
    data: Vec<JsWord>,
    setup: Vec<JsWord>
}

pub fn analyze_script_legacy(module: &Module) {
    // TODO Be more tolerant to things missing (e.g. setup(), data(), etc.)

    // Where should we collect our stuff
    let mut script_legacy_vars = ScriptLegacyVars::default();

    let Some(default_export) = find_default_export(module) else {
        return;
    };

    if let Some(setup_fn) = find_setup_function(default_export) {
        collect_fn_return_fields(setup_fn, &mut script_legacy_vars.setup);
    }

    if let Some(data_fn) = find_data_function(default_export) {
        collect_fn_return_fields(data_fn, &mut script_legacy_vars.data);
    };

    // TODO use `find_function` and individually analyze `data`, `setup`, `props`

    // TODO Do not go too deep and crazy and use utils instead

    for field in script_legacy_vars.setup {
        println!("SETUP: {:?}", field);
    }

    for field in script_legacy_vars.data {
        println!("DATA: {:?}", field);
    }

    // println!("Returned {:?}", return_obj);
}

// pub fn transform_script_legacy() {
//     todo!()
// }
