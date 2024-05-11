use fervid_core::{FervidAtom, SfcTemplateBlock, TemplateGenerationMode};
use swc_core::{
    atoms::Atom,
    common::{
        collections::AHashMap, source_map::SourceMapGenConfig, BytePos, FileName, SourceMap,
        DUMMY_SP,
    },
    ecma::{
        ast::{
            ArrowExpr, BindingIdent, BlockStmt, BlockStmtOrExpr, Decl, ExportDefaultExpr, Expr,
            Function, Ident, ImportDecl, MethodProp, Module, ModuleDecl, ModuleItem, ObjectLit,
            Param, Pat, Prop, PropName, PropOrSpread, ReturnStmt, Stmt, Str, VarDecl, VarDeclKind,
            VarDeclarator,
        },
        visit::{noop_visit_type, Visit, VisitWith},
    },
};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node};

use crate::context::CodegenContext;

impl CodegenContext {
    // TODO Generation mode? Is it relevant?
    // TODO Generating module? Or instead taking a module? Or generating an expression and merging?
    pub fn generate_sfc_template(&mut self, sfc_template: &SfcTemplateBlock) -> Option<Expr> {
        // #11: Optimization: multiple template roots
        // and all are text nodes (must be ensured by Transformer),
        // generate node sequence
        if sfc_template.roots.len() > 1 {
            let mut out = Vec::new();
            self.generate_node_sequence(
                &mut sfc_template.roots.iter(),
                &mut out,
                sfc_template.roots.len(),
                true,
            );

            out.pop()
        } else if sfc_template.roots.len() == 1 {
            // Generate the only child
            let first_child = &sfc_template.roots[0];
            Some(self.generate_node(&first_child, true))
        } else {
            None
        }
    }

    pub fn generate_module(
        &mut self,
        template_expr: Option<Expr>,
        mut script: Module,
        mut sfc_export_obj: ObjectLit,
        mut synthetic_setup_fn: Option<Box<Function>>,
        gen_default_as: Option<&str>,
    ) -> Module {
        let template_generation_mode = &self.bindings_helper.template_generation_mode;

        if let Some(template_expr) = template_expr {
            match template_generation_mode {
                // Generates the render expression and appends it to the end of the `setup` function.
                TemplateGenerationMode::Inline => {
                    let render_arrow = self.generate_render_arrow(template_expr);

                    let setup_function = synthetic_setup_fn.get_or_insert_with(|| {
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

                    // When a synthetic setup function is present,
                    // we need to return bindings as its last statement
                    'return_bindings: {
                        let Some(ref mut setup_fn) = synthetic_setup_fn else {
                            break 'return_bindings;
                        };

                        let Some(ref mut setup_body) = setup_fn.body else {
                            break 'return_bindings;
                        };

                        let return_bindings = self.generate_return_bindings();
                        if !return_bindings.props.is_empty() {
                            setup_body.stmts.push(Stmt::Return(ReturnStmt {
                                span: DUMMY_SP,
                                arg: Some(Box::new(Expr::Object(return_bindings))),
                            }));
                        }
                    }

                    sfc_export_obj
                        .props
                        .push(PropOrSpread::Prop(Box::new(Prop::Method(MethodProp {
                            key: PropName::Ident(Ident {
                                span: DUMMY_SP,
                                sym: FervidAtom::from("render"),
                                optional: false,
                            }),
                            function: Box::new(render_fn),
                        }))));
                }
            }
        }

        // Add the `setup` function to the exported object
        if let Some(setup_fn) = synthetic_setup_fn {
            match setup_fn.body {
                // Append only when function has a body and it is not empty
                Some(ref b) if !b.stmts.is_empty() => {
                    sfc_export_obj
                        .props
                        .push(PropOrSpread::Prop(Box::new(Prop::Method(MethodProp {
                            key: PropName::Ident(Ident {
                                span: DUMMY_SP,
                                sym: FervidAtom::from("setup"),
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
                        value: FervidAtom::from("vue"),
                        raw: None,
                    }),
                    type_only: false,
                    with: None,
                    phase: Default::default(),
                })));
        }

        let gen_default_as = if let Some(options_gen_default_as) = gen_default_as {
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                span: DUMMY_SP,
                kind: VarDeclKind::Const,
                declare: false,
                decls: vec![VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent {
                        id: Ident {
                            span: DUMMY_SP,
                            sym: FervidAtom::from(options_gen_default_as),
                            optional: false,
                        },
                        type_ann: None,
                    }),
                    init: Some(Box::new(Expr::Object(sfc_export_obj))),
                    definite: false,
                }],
            }))))
        } else {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(ExportDefaultExpr {
                span: DUMMY_SP,
                expr: Box::new(Expr::Object(sfc_export_obj)),
            }))
        };

        // Append the default export/const
        script.body.push(gen_default_as);

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

            Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                span: DUMMY_SP,
                stmts,
            }))
        };

        macro_rules! param {
            ($ident: expr) => {
                Pat::Ident(BindingIdent {
                    id: Ident {
                        span: DUMMY_SP,
                        sym: FervidAtom::from($ident),
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
                            sym: FervidAtom::from($ident),
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

    /// Generates bindings for a synthetic setup function when used in combination
    /// with `TemplateGenerationMode::RenderFn`.
    pub fn generate_return_bindings(&self) -> ObjectLit {
        let mut props =
            Vec::<PropOrSpread>::with_capacity(self.bindings_helper.used_bindings.len());

        for used_binding in self.bindings_helper.used_bindings.keys() {
            props.push(PropOrSpread::Prop(Box::new(Prop::Shorthand(Ident {
                span: DUMMY_SP,
                sym: used_binding.to_owned(),
                optional: false,
            }))));
        }

        ObjectLit {
            span: DUMMY_SP,
            props,
        }
    }

    pub fn stringify<T>(
        source: &str,
        module: &T,
        filename: FileName,
        generate_source_map: bool,
        minify: bool,
    ) -> (String, Option<String>)
    where
        T: Node + VisitWith<IdentCollector>,
    {
        // Emitting the result requires some setup with SWC
        let cm: swc_core::common::sync::Lrc<SourceMap> = Default::default();
        cm.new_source_file(filename.to_owned(), source.to_owned());

        let mut source_map_buf = vec![];

        let generated = {
            let mut buff: Vec<u8> = Vec::new();
            let src_map = if generate_source_map {
                Some(&mut source_map_buf)
            } else {
                None
            };
            let writer: JsWriter<&mut Vec<u8>> =
                JsWriter::new(cm.clone(), "\n", &mut buff, src_map);

            let mut emitter_cfg = swc_ecma_codegen::Config::default();
            emitter_cfg.minify = minify;

            let mut emitter = Emitter {
                cfg: emitter_cfg,
                comments: None,
                wr: writer,
                cm: cm.clone(),
            };

            module.emit_with(&mut emitter).expect("Failed to emit");
            String::from_utf8(buff).expect("Invalid UTF-8")
        };

        let map = if generate_source_map {
            let source_map_names = {
                let mut v = IdentCollector {
                    names: Default::default(),
                };

                module.visit_with(&mut v);

                v.names
            };

            let map = cm.build_source_map_with_config(
                &source_map_buf,
                None,
                SourceMapConfig {
                    source_file_name: Some(filename.to_string().as_str()),
                    names: &source_map_names,
                },
            );
            let mut buf = vec![];

            map.to_writer(&mut buf).expect("Failed to write source map");
            Some(String::from_utf8(buf).expect("Invalid UTF-8 in source map"))
        } else {
            None
        };

        (generated, map)
    }
}

struct SourceMapConfig<'a> {
    source_file_name: Option<&'a str>,
    names: &'a AHashMap<BytePos, FervidAtom>,
}

impl SourceMapGenConfig for SourceMapConfig<'_> {
    fn file_name_to_source(&self, f: &FileName) -> String {
        if let Some(file_name) = self.source_file_name {
            return file_name.to_string();
        }

        f.to_string()
    }

    fn inline_sources_content(&self, _f: &FileName) -> bool {
        true
    }

    fn name_for_bytepos(&self, pos: BytePos) -> Option<&str> {
        self.names.get(&pos).map(|v| &**v)
    }
}

// Adapted from `swc_compiler_base`
pub struct IdentCollector {
    pub names: AHashMap<BytePos, Atom>,
}

impl Visit for IdentCollector {
    noop_visit_type!();

    fn visit_ident(&mut self, ident: &Ident) {
        self.names.insert(ident.span.lo, ident.sym.clone());
    }
}
