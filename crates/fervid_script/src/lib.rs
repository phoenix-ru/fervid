#[macro_use]
extern crate lazy_static;

pub mod atoms;
pub mod common;
pub mod parser;
pub mod script_legacy;
pub mod script_setup;
pub mod setup_analyzer;
pub mod structs;

// use std::sync::Arc;

// use swc_core::common::{FileName, SourceMap};
// use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node as _};

// mod experimental_compile;

// pub fn read_script(input: &str) -> Result<String, ()> {
//     let parse_result = parser::parse_typescript_module(input, 0, Default::default());

//     let parsed = match parse_result {
//         Ok(module) => module,
//         Err(e) => {
//             eprintln!("{:?}", e.kind());
//             return Err(());
//         }
//     };

//     let (module, comments) = parsed;

//     let analysis_res = analyze_script_legacy(&module);
//     if let Ok(analyzed) = analysis_res {
//         for field in analyzed.setup.iter() {
//             println!("SETUP: {:?}", field);
//         }

//         for field in analyzed.data.iter() {
//             println!("DATA: {:?}", field);
//         }

//         for field in analyzed.props.iter() {
//             println!("PROPS: {:?}", field);
//         }
//     }

//     // Create and invoke the visitor
//     // let mut visitor = TransformVisitor {
//     //     current_scope: scope_to_use,
//     //     scope_helper
//     // };
//     // parsed.visit_mut_with(&mut visitor);

//     // Emitting the result requires some setup with SWC
//     let cm: Arc<SourceMap> = Default::default();
//     let src = input.to_owned();
//     cm.new_source_file(FileName::Custom("test.ts".to_owned()), src);
//     let mut buff: Vec<u8> = Vec::with_capacity(input.len() * 2);
//     let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

//     let mut emitter = Emitter {
//         cfg: swc_ecma_codegen::Config {
//             target: Default::default(),
//             ascii_only: false,
//             minify: false,
//             omit_last_semi: false,
//         },
//         comments: Some(&comments),
//         wr: writer,
//         cm,
//     };

//     let _ = module.emit_with(&mut emitter);

//     String::from_utf8(buff).map_err(|_| ())
// }
