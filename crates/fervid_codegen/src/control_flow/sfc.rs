use fervid_core::{SfcTemplateBlock, TemplateGenerationMode};
use swc_core::{
    common::{FileName, SourceMap, DUMMY_SP},
    ecma::{
        ast::{
            BindingIdent, BlockStmt, Decl, ExportDefaultExpr, Expr, Function,
            Ident, ImportDecl, MethodProp, Module, ModuleDecl, ModuleItem, ObjectLit, Param, Pat,
            Prop, PropName, PropOrSpread, ReturnStmt, Stmt, Str, VarDecl, VarDeclKind, ArrowExpr, BlockStmtOrExpr,
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
        self.generate_node(&first_child, true)
    }

    pub fn generate_module(
        &mut self,
        template_expr: Option<Expr>,
        mut script: Module,
        mut sfc_export_obj: ObjectLit,
        mut setup_fn: Option<Box<Function>>,
        template_generation_mode: TemplateGenerationMode,
    ) -> Module {
        if let Some(template_expr) = template_expr {
            match template_generation_mode {
                // Generates the render expression and appends it to the end of the `setup` function.
                TemplateGenerationMode::Inline => {
                    let render_arrow = self.generate_render_arrow(template_expr);

                    let setup_function = setup_fn.get_or_insert_with(|| {
                        Box::new(Function {
                            params: vec![],
                            decorators: vec![],
                            span: DUMMY_SP,
                            body: None,
                            is_generator: false,
                            is_async: false,
                            type_params: None,
                            return_type: None,
                        })
                    });

                    let setup_body = setup_function.body.get_or_insert_with(|| BlockStmt {
                        span: DUMMY_SP,
                        stmts: Vec::with_capacity(1),
                    });

                    setup_body.stmts.push(Stmt::Return(ReturnStmt {
                        span: DUMMY_SP,
                        arg: Some(Box::new(Expr::Arrow(render_arrow))),
                    }));
                }

                // Generates the render Function to be used as a property in exported object.
                // `render(_ctx, _cache, $props, $setup, $data, $options) { /*...*/ }`
                TemplateGenerationMode::RenderFn => {
                    let render_fn = self.generate_render_fn(template_expr);

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
            }
        }

        // Add the `setup` function to the exported object
        if let Some(setup_fn) = setup_fn {
            match setup_fn.body {
                // Append only when function has a body and it is not empty
                Some(ref b) if !b.stmts.is_empty() => {
                    sfc_export_obj
                        .props
                        .push(PropOrSpread::Prop(Box::new(Prop::Method(MethodProp {
                            key: PropName::Ident(Ident {
                                span: DUMMY_SP,
                                sym: JsWord::from("setup"),
                                optional: false,
                            }),
                            function: setup_fn,
                        }))));
                }

                _ => {}
            }
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
                    with: None,
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

    /// Wraps the render function in an arrow expression
    ///
    /// `(_ctx, _cache) => { /*...*/ }` or `(_ctx, _cache) => /*...*/`
    pub fn generate_render_arrow(&mut self, template_expr: Expr) -> ArrowExpr {
        // Compute component and directive resolves
        let mut component_resolves = self.generate_component_resolves();
        let directive_resolves = self.generate_directive_resolves();

        let body = if directive_resolves.is_empty() && component_resolves.is_empty() {
            // We can directly return an expression from an arrow function
            Box::new(BlockStmtOrExpr::Expr(Box::new(template_expr)))
        } else {
            //  We need to return a block for an arrow function
            let mut stmts: Vec<Stmt> = Vec::with_capacity(2);
            component_resolves.extend(directive_resolves);

            // Add resolves
            stmts.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                span: DUMMY_SP,
                kind: VarDeclKind::Const,
                declare: false,
                decls: component_resolves,
            }))));

            // Add template expression return
            stmts.push(Stmt::Return(ReturnStmt {
                arg: Some(Box::new(template_expr)),
                span: DUMMY_SP,
            }));

            Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt { span: DUMMY_SP, stmts }))
        };

        macro_rules! param {
            ($ident: expr) => {
                Pat::Ident(BindingIdent {
                    id: Ident {
                        span: DUMMY_SP,
                        sym: JsWord::from($ident),
                        optional: false,
                    },
                    type_ann: None,
                })
            };
        }

        ArrowExpr {
            span: DUMMY_SP,
            params: vec![param!("_ctx"), param!("_cache")],
            body,
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        }
    }

    /// Wraps the render function in a `Function`.
    ///
    /// It always includes the provided `template_expr` as the last return statement.
    /// When components and/or directives are present, their corresponding `resolve`s are generated here.
    pub fn generate_render_fn(&mut self, template_expr: Expr) -> Function {
        let mut fn_body_stmts: Vec<Stmt> = Vec::with_capacity(3);

        // Compute component and directive resolves
        let mut component_resolves = self.generate_component_resolves();
        let directive_resolves = self.generate_directive_resolves();

        // Add them
        if !directive_resolves.is_empty() || !component_resolves.is_empty() {
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

        let mut emitter_cfg = swc_ecma_codegen::Config::default();
        emitter_cfg.minify = minify;

        let mut emitter = Emitter {
            cfg: emitter_cfg,
            comments: None,
            wr: writer,
            cm,
        };

        let _ = item.emit_with(&mut emitter);

        String::from_utf8(buff).unwrap()
    }
}
