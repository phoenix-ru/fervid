use std::sync::Arc;

use script_legacy::analyze_script_legacy;
use swc_core::common::{FileName, SourceMap};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node as _};

mod parser;
mod script_legacy;
mod script_setup;

pub use parser::*;

pub fn read_script(input: &str) -> Result<String, ()> {
    let parse_result = parser::parse_typescript_module(input, 0, Default::default());

    let parsed = match parse_result {
        Ok(module) => module,
        Err(e) => {
            eprintln!("{:?}", e.kind());
            return Err(());
        }
    };

    analyze_script_legacy(&parsed);

    // Create and invoke the visitor
    // let mut visitor = TransformVisitor {
    //     current_scope: scope_to_use,
    //     scope_helper
    // };
    // parsed.visit_mut_with(&mut visitor);

    // Emitting the result requires some setup with SWC
    let cm: Arc<SourceMap> = Default::default();
    let src = input.to_owned();
    cm.new_source_file(FileName::Custom("test.ts".to_owned()), src);
    let mut buff: Vec<u8> = Vec::with_capacity(input.len() * 2);
    let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

    let mut emitter = Emitter {
        cfg: swc_ecma_codegen::Config {
            target: Default::default(),
            ascii_only: false,
            minify: false,
            omit_last_semi: false,
        },
        comments: None,
        wr: writer,
        cm,
    };

    let _ = parsed.emit_with(&mut emitter);

    String::from_utf8(buff).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        // TODO Write tests to check that all the needed values were scraped
        // Check cases with/without `defineComponent` and without `export default`

        let result = read_script(
            r#"
        import { defineComponent, ref } from 'vue'

        export default defineComponent({
            data() {
                return {
                    hello: 'world'
                }
            },
            setup() {
                const inputModel = ref('')
                const modelValue = ref('')
                const list = [1, 2, 3]

                return {
                    inputModel,
                    modelValue,
                    list
                }
            },
        })
        "#,
        );
        assert!(result.is_ok());

        println!("{}", result.unwrap())
    }
}
