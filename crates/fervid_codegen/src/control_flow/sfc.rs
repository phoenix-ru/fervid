use std::sync::Arc;

use fervid_core::SfcTemplateBlock;
use swc_core::{
    common::{SourceMap, DUMMY_SP},
    ecma::ast::{Expr, ExprStmt, Module, ModuleItem, Stmt},
};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};

use crate::context::CodegenContext;

impl CodegenContext {
    // TODO Generation mode? Is it relevant?
    // TODO Generating module? Or instead taking a module? Or generating an expression and merging?
    pub fn generate_sfc_template(&mut self, sfc_template: &SfcTemplateBlock) -> Expr {
        assert!(!sfc_template.roots.is_empty());

        // TODO Multi-root? Is it actually merged before into a Fragment?
        let first_child = &sfc_template.roots[0];
        let (result, _) = self.generate_node(&first_child, true);

        result
    }

    pub fn generate_module(&mut self, template_expr: Expr, mut script: Module) -> Module {
        // TODO Properly append the template code depending on mode, what scripts are there, etc.
        script.body.push(ModuleItem::Stmt(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(template_expr),
        })));

        script
    }

    pub fn stringify(item: &impl Node, minify: bool) -> String {
        // Emitting the result requires some setup with SWC
        let cm: Arc<SourceMap> = Default::default();
        let mut buff: Vec<u8> = Vec::new();
        let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config {
                target: Default::default(),
                ascii_only: false,
                minify,
                omit_last_semi: false,
            },
            comments: None,
            wr: writer,
            cm,
        };

        let _ = item.emit_with(&mut emitter);

        String::from_utf8(buff).unwrap()
    }
}
