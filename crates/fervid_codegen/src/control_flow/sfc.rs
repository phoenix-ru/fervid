use fervid_core::SfcTemplateBlock;
use swc_core::{
    common::{FileName, SourceMap, DUMMY_SP},
    ecma::{
        ast::{
            BindingIdent, BlockStmt, Decl, ExportDefaultExpr, Expr, Function, Ident, ImportDecl,
            MethodProp, Module, ModuleDecl, ModuleItem, ObjectLit, Param, Pat, Prop, PropName,
            PropOrSpread, ReturnStmt, Stmt, Str, VarDecl, VarDeclKind,
        },
        atoms::JsWord,
    },
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

    pub fn generate_module(
        &mut self,
        template_expr: Option<Expr>,
        mut script: Module,
        mut sfc_export_obj: ObjectLit,
    ) -> Module {
        if let Some(template_expr) = template_expr {
            // TODO Distinguish between RenderFn and Inline
            let render_fn = self.generate_render_fn(template_expr);
    
            // TODO Properly append the template code depending on mode, what scripts are there, etc.
            // `render(_ctx) { return template_expression }`
            sfc_export_obj
                .props
                .push(PropOrSpread::Prop(Box::new(Prop::Method(MethodProp {
                    key: PropName::Ident(Ident {
                        span: DUMMY_SP,
                        sym: JsWord::from("render"),
                        optional: false,
                    }),
                    function: Box::new(render_fn),
                }))));
        }

        // Append the Vue imports
        // TODO Smart merging with user imports?
        let used_imports = self.generate_imports();
        if !used_imports.is_empty() {
            script
                .body
                .push(ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
                    span: DUMMY_SP,
                    specifiers: used_imports,
                    src: Box::new(Str {
                        span: DUMMY_SP,
                        value: JsWord::from("vue"),
                        raw: None,
                    }),
                    type_only: false,
                    asserts: None,
                })));
        }

        // Append the default export
        script
            .body
            .push(ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(
                ExportDefaultExpr {
                    span: DUMMY_SP,
                    expr: Box::new(Expr::Object(sfc_export_obj)),
                },
            )));

        script
    }

    pub fn generate_render_fn(&mut self, template_expr: Expr) -> Function {
        let mut fn_body_stmts: Vec<Stmt> = Vec::with_capacity(3);

        // Compute component and directive resolves
        let mut component_resolves = self.generate_component_resolves();
        let directive_resolves = self.generate_directive_resolves();

        // Add them
        if directive_resolves.len() != 0 || component_resolves.len() != 0 {
            component_resolves.extend(directive_resolves);

            fn_body_stmts.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                span: DUMMY_SP,
                kind: VarDeclKind::Const,
                declare: false,
                decls: component_resolves,
            }))));
        }

        // Add template expression return
        fn_body_stmts.push(Stmt::Return(ReturnStmt {
            arg: Some(Box::new(template_expr)),
            span: DUMMY_SP,
        }));

        macro_rules! param {
            ($ident: expr) => {
                Param {
                    span: DUMMY_SP,
                    decorators: vec![],
                    pat: Pat::Ident(BindingIdent {
                        id: Ident {
                            span: DUMMY_SP,
                            sym: JsWord::from($ident),
                            optional: false,
                        },
                        type_ann: None,
                    }),
                }
            };
        }

        Function {
            // Render function params
            params: vec![
                param!("_ctx"),
                param!("_cache"),
                param!("$props"),
                param!("$setup"),
                param!("$data"),
                param!("$options"),
            ],
            decorators: vec![],
            span: DUMMY_SP,
            body: Some(BlockStmt {
                span: DUMMY_SP,
                stmts: fn_body_stmts,
            }),
            is_generator: false,
            is_async: false,
            type_params: None,
            return_type: None,
        }
    }

    pub fn stringify(source: &str, item: &impl Node, minify: bool) -> String {
        // Emitting the result requires some setup with SWC
        let cm: swc_core::common::sync::Lrc<SourceMap> = Default::default();
        cm.new_source_file(FileName::Custom("test.ts".to_owned()), source.to_owned());
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
