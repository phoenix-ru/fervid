use fervid_core::{is_valid_propname, BindingTypes, FervidAtom, IntoIdent};
use fxhash::{FxHashMap, FxHashSet};
use swc_core::{
    common::{BytePos, Span, Spanned, DUMMY_SP},
    ecma::{
        ast::{
            ArrayPat, AssignTarget, AssignTargetPat, BlockStmt, BlockStmtOrExpr, CallExpr, Callee,
            CatchClause, ClassDecl, ComputedPropName, Decl, Expr, ExprOrSpread, FnDecl, ForHead,
            ForInStmt, ForOfStmt, Function, Ident, IdentName, KeyValuePatProp, KeyValueProp, Lit,
            MemberExpr, MemberProp, ModuleDecl, ModuleItem, ObjectLit, ObjectPat, ObjectPatProp,
            Pat, Prop, PropName, PropOrSpread, SimpleAssignTarget, Stmt, Str, VarDecl,
            VarDeclOrExpr,
        },
        visit::{VisitMut, VisitMutWith},
    },
};

use crate::{
    atoms::{DEFINE_PROPS, PROPS_HELPER, TO_REF, WATCH},
    error::{ScriptError, ScriptErrorKind, TransformError},
    script::{
        common::extract_variables_from_pat,
        resolve_type::TypeResolveContext,
        utils::{is_call_of, resolve_object_key},
    },
    PropsDestructureBinding, PropsDestructureConfig, SetupBinding, VueImportAliases,
};

use super::utils::unwrap_ts_node_expr;

// TODO This is a difference with the official compiler:
// - official compiler does separate collection (called `process`) and processing (called `extract`) loops;
// - it works fine because AST is not really being manipulated and instead being referenced everywhere;
// Fervid cannot afford this, because it does collection-processing on the same loop
// Fervid can still do pre-processing though by collecting variables before the main processing
// For props destructure it means collecting bindings with their default values and then doing a separate loop to transform (not necessarily in pre-processing stage, but before the main processing)

pub fn collect_props_destructure(
    ctx: &mut TypeResolveContext,
    obj_pat: &ObjectPat,
    errors: &mut Vec<TransformError>,
) {
    match ctx.props_destructure {
        PropsDestructureConfig::False => return,
        PropsDestructureConfig::True => {}
        PropsDestructureConfig::Error => {
            errors.push(TransformError::ScriptError(ScriptError {
                span: obj_pat.span,
                kind: ScriptErrorKind::DefinePropsDestructureForbidden,
            }));
            return;
        }
    }

    /// https://github.com/vuejs/core/blob/466b30f4049ec89fb282624ec17d1a93472ab93f/packages/compiler-sfc/src/script/definePropsDestructure.ts#L39
    fn register_binding(
        ctx: &mut TypeResolveContext,
        key: &FervidAtom,
        local: &FervidAtom,
        default_value: Option<Box<Expr>>,
    ) {
        if local != key {
            ctx.bindings_helper.setup_bindings.push(SetupBinding::new(
                local.to_owned(),
                BindingTypes::PropsAliased,
                // TODO span?
            ));

            ctx.bindings_helper
                .props_aliases
                .insert(local.to_owned(), key.to_owned());
        }

        ctx.bindings_helper.props_destructured_bindings.insert(
            key.to_owned(),
            PropsDestructureBinding {
                local: local.to_owned(),
                default: default_value,
            },
        );
    }

    // https://github.com/vuejs/core/blob/466b30f4049ec89fb282624ec17d1a93472ab93f/packages/compiler-sfc/src/script/definePropsDestructure.ts#L52-L89
    for prop in obj_pat.props.iter() {
        match prop {
            // Covers `const { foo: bar } = defineProps()`
            ObjectPatProp::KeyValue(key_value_pat_prop) => {
                let Some(prop_key) = resolve_object_key(&key_value_pat_prop.key) else {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: key_value_pat_prop.span(),
                        kind: ScriptErrorKind::DefinePropsDestructureCannotUseComputedKey,
                    }));
                    continue;
                };

                let mut default_value = None;

                let ident = match key_value_pat_prop.value.as_ref() {
                    // const { foo: bar }
                    Pat::Ident(binding_ident) => Some(binding_ident),

                    Pat::Assign(assign_pat) => {
                        // const { foo: bar = 'baz' }
                        if let Pat::Ident(ident) = assign_pat.left.as_ref() {
                            default_value = Some(assign_pat.right.to_owned());
                            Some(ident)
                        } else {
                            None
                        }
                    }

                    _ => None,
                };

                let Some(ident) = ident else {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: key_value_pat_prop.span(),
                        kind: ScriptErrorKind::DefinePropsDestructureUnsupportedNestedPattern,
                    }));
                    continue;
                };

                register_binding(ctx, &prop_key, &ident.sym, default_value);
            }

            // Covers `const { foo = bar }` and `const { foo }`
            ObjectPatProp::Assign(assign_pat_prop) => {
                let prop_key = &assign_pat_prop.key.sym;
                register_binding(ctx, prop_key, prop_key, assign_pat_prop.value.to_owned());
            }

            // Covers `rest` property in `const { foo, ...rest }`
            ObjectPatProp::Rest(rest_pat) => {
                let Some(rest_pat_name) = rest_pat.arg.as_ident() else {
                    errors.push(TransformError::ScriptError(ScriptError {
                        span: rest_pat.span,
                        kind: ScriptErrorKind::DefinePropsDestructureUnsupportedNestedPattern,
                    }));
                    continue;
                };

                let key = rest_pat_name.id.sym.to_owned();

                ctx.bindings_helper.props_destructure_rest_id = Some(key.to_owned());

                ctx.bindings_helper
                    .setup_bindings
                    .push(SetupBinding::new_spanned(
                        key,
                        BindingTypes::SetupReactiveConst,
                        Span::new(BytePos(0), BytePos(0)),
                    ));
            }
        }
    }
}

pub fn transform_destructured_props(
    ctx: &mut TypeResolveContext,
    setup_stmts: &mut Vec<Stmt>,
    module_items: &mut Vec<ModuleItem>,
    errors: &mut Vec<TransformError>,
) {
    if matches!(ctx.props_destructure, PropsDestructureConfig::False) {
        return;
    }

    let mut walker = Walker::new(ctx, errors);

    // Visit the root first - only collect
    walker.is_root = true;
    walker.collect_stmts(setup_stmts);
    walker.collect_module_items(module_items);

    // Visit the remainder - transform this time
    walker.is_root = false;
    walker.transform_stmts(setup_stmts);
    walker.transform_module_items(module_items);

    #[cfg(test)]
    dbg!(&walker.all_scopes);
}

struct Walker<'a> {
    all_scopes: Vec<Scope>,
    current_scope: usize,
    errors: &'a mut Vec<TransformError>,
    excluded_ids: FxHashSet<Identifier>,
    props_local_to_public_map: FxHashMap<FervidAtom, FervidAtom>,
    stmt_visit_mode: StmtVisitMode,
    is_root: bool,
    is_in_assign_target: bool,
    is_in_destructure_assign: bool,
    vue_import_aliases: &'a VueImportAliases,
}

#[derive(Clone, Copy)]
enum StmtVisitMode {
    Collect,
    Transform,
}

#[derive(Debug, Default)]
struct Scope {
    parent_scope: usize, // 0 means root scope
    // true - prop binding
    // false - local binding
    variables: FxHashMap<FervidAtom, bool>,
}

/// Special struct for mirroring `excludedIds` which stores Babel `Identifier`s
#[derive(Hash, PartialEq, Eq, Clone)]
struct Identifier(FervidAtom, Span);

impl<'a> Walker<'a> {
    fn new(ctx: &'a mut TypeResolveContext, errors: &'a mut Vec<TransformError>) -> Self {
        let mut walker = Self {
            all_scopes: vec![Scope::default()],
            current_scope: 0,
            errors,
            excluded_ids: FxHashSet::default(),
            props_local_to_public_map: FxHashMap::default(),
            stmt_visit_mode: StmtVisitMode::Collect,
            is_root: false,
            is_in_assign_target: false,
            is_in_destructure_assign: false,
            vue_import_aliases: ctx.bindings_helper.vue_import_aliases.as_ref(),
        };

        // Fill the root scope
        let root_scope = &mut walker.all_scopes[0];

        for (key, binding) in ctx.bindings_helper.props_destructured_bindings.iter() {
            root_scope.variables.insert(binding.local.to_owned(), true);
            walker
                .props_local_to_public_map
                .insert(binding.local.to_owned(), key.to_owned());
        }

        walker
    }

    fn collect_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let old_visit_mode = self.stmt_visit_mode;

        self.stmt_visit_mode = StmtVisitMode::Collect;
        for stmt in stmts.iter_mut() {
            stmt.visit_mut_with(self);
        }

        self.stmt_visit_mode = old_visit_mode;
    }

    fn transform_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let old_visit_mode = self.stmt_visit_mode;

        self.stmt_visit_mode = StmtVisitMode::Transform;
        for stmt in stmts.iter_mut() {
            stmt.visit_mut_with(self);
        }

        self.stmt_visit_mode = old_visit_mode;
    }

    // This function mimics the behavior of the official compiler
    // which first collects all the references in the scope and only after transforms them.
    fn collect_then_transform_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        self.collect_stmts(stmts);
        self.transform_stmts(stmts);
    }

    fn collect_module_items(&mut self, module_items: &mut Vec<ModuleItem>) {
        let old_visit_mode = self.stmt_visit_mode;

        self.stmt_visit_mode = StmtVisitMode::Collect;

        for module_item in module_items.iter_mut() {
            match module_item {
                ModuleItem::ModuleDecl(module_decl) => {
                    match module_decl {
                        // Same as `ExportNamedDeclaration` in Babel
                        ModuleDecl::ExportDecl(export_decl) => {
                            if let Decl::Var(ref var_decl) = export_decl.decl {
                                self.collect_variable_declaration(var_decl);
                            }
                        }

                        _ => {}
                    }
                }
                ModuleItem::Stmt(stmt) => {
                    stmt.visit_mut_with(self);
                }
            }
        }

        self.stmt_visit_mode = old_visit_mode;
    }

    fn transform_module_items(&mut self, module_items: &mut Vec<ModuleItem>) {
        let old_visit_mode = self.stmt_visit_mode;

        self.stmt_visit_mode = StmtVisitMode::Transform;

        for module_item in module_items.iter_mut() {
            module_item.visit_mut_with(self);
        }

        self.stmt_visit_mode = old_visit_mode;
    }

    fn collect_variable_declaration(&mut self, var_decl: &VarDecl) {
        if var_decl.declare {
            return;
        }

        let is_root = self.is_root;

        // Technically unneeded
        let is_const = matches!(var_decl.kind, swc_core::ecma::ast::VarDeclKind::Const);

        // Re-use one array to avoid extra allocations
        let mut identifiers = vec![];

        for decl in var_decl.decls.iter() {
            let is_define_props = is_root && {
                if let Some(ref decl_init) = decl.init {
                    is_call_of(unwrap_ts_node_expr(&decl_init), &DEFINE_PROPS)
                } else {
                    false
                }
            };

            extract_variables_from_pat(&decl.name, &mut identifiers, is_const);

            for id in identifiers.drain(..) {
                let identifier = Identifier(id.sym, id.span);
                if is_define_props {
                    self.excluded_ids.insert(identifier);
                } else {
                    self.register_local_binding(identifier);
                }
            }
        }
    }

    fn register_local_binding(&mut self, identifier: Identifier) {
        self.excluded_ids.insert(identifier.to_owned());

        let current_scope = self.get_current_scope();
        current_scope.variables.insert(identifier.0, false);
    }

    fn push_scope(&mut self) {
        let new_scope_id = self.all_scopes.len();
        self.all_scopes.push(Scope {
            parent_scope: self.current_scope,
            variables: FxHashMap::default(),
        });
        self.current_scope = new_scope_id;
    }

    fn pop_scope(&mut self) {
        let old_current_scope = self.get_current_scope();
        self.current_scope = old_current_scope.parent_scope;
    }

    fn get_current_scope(&mut self) -> &mut Scope {
        if let Some(_) = self.all_scopes.get(self.current_scope) {
            return &mut self.all_scopes[self.current_scope];
        }

        // Default to root scope - it is always present
        &mut self.all_scopes[0]
    }

    fn check_usage(&mut self, call_expr: &CallExpr) {
        let Callee::Expr(ref callee_expr) = call_expr.callee else {
            return;
        };

        let Expr::Ident(ref callee_ident) = callee_expr.as_ref() else {
            return;
        };

        // First argument of CallExpr needs to exist and be an expression (not a spread)
        let Some(ExprOrSpread {
            spread: None,
            expr: ref first_arg,
        }) = call_expr.args.first()
        else {
            return;
        };

        let to_ref = self
            .vue_import_aliases
            .to_ref
            .as_ref()
            .map(|v| &v.0)
            .unwrap_or(&TO_REF);

        let watch = self
            .vue_import_aliases
            .watch
            .as_ref()
            .map(|v| &v.0)
            .unwrap_or(&WATCH);

        let is_to_ref = &callee_ident.sym == to_ref;
        let is_watch = &callee_ident.sym == watch;

        if !is_to_ref && !is_watch {
            return;
        }

        // First argument needs to be an identifier
        let Expr::Ident(first_arg_ident) = unwrap_ts_node_expr(&first_arg) else {
            return;
        };

        // When the first argument of `watch` or `toRef` is an identifier
        // which points to a destructured prop, emit an error (since it needs to be a getter instead)
        if self.should_rewrite(&first_arg_ident.sym) {
            self.errors.push(TransformError::ScriptError(ScriptError {
                span: first_arg_ident.span,
                kind: if is_to_ref {
                    ScriptErrorKind::DefinePropsDestructureShouldNotPassToToRef
                } else {
                    ScriptErrorKind::DefinePropsDestructureShouldNotPassToWatch
                },
            }));
        }
    }

    fn should_rewrite(&self, sym: &FervidAtom) -> bool {
        let mut current_scope = self.current_scope;

        while let Some(scope) = self.all_scopes.get(current_scope) {
            if let Some(value) = scope.variables.get(sym) {
                // true -> needs rewrite
                // false -> does not need
                return *value;
            }

            // End of iteration - root scope already reached
            if current_scope == 0 && scope.parent_scope == 0 {
                break;
            }

            current_scope = scope.parent_scope;
        }

        return false;
    }

    fn should_rewrite_with(&self, ident: &Ident) -> Option<MemberProp> {
        let sym = &ident.sym;

        if !self.should_rewrite(sym) {
            return None;
        }

        let Some(found) = self.props_local_to_public_map.get(sym) else {
            return None; // should not be the case
        };

        // `__props.foo` or `__props['foo']` depending on ident validity
        if is_valid_propname(&found) {
            // Preserve original span if rewrite with == rewrite to
            let span = if found == sym { ident.span } else { DUMMY_SP };

            Some(MemberProp::Ident(IdentName {
                span,
                sym: found.to_owned(),
            }))
        } else {
            Some(MemberProp::Computed(ComputedPropName {
                span: DUMMY_SP,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    raw: None,
                    span: DUMMY_SP,
                    value: found.to_owned(),
                }))),
            }))
        }
    }
}

// Adapted from template::expr_transform
impl<'a> VisitMut for Walker<'a> {
    fn visit_mut_stmt(&mut self, stmt: &mut Stmt) {
        // TODO When in statement:
        // if root - only visit the LHS, delay till all statements are processed, then only visit the RHS
        // if not root - visit both
        //
        // How to do this - use something like `visit_mut_module` (e.g. module items / separate function with a for-loop)
        // set `is_root: true`
        // for stmt in stmts
        //   visit_lhs() -> custom visit function or use visit_var_decl
        // for stmt in stmts
        //   visit_rhs()

        // This walks the Stmt for the sake of collecting bindings.
        // It doesn't actually visit the RHS or transform anything.
        // https://github.com/vuejs/core/blob/32bc647faba56f50a37d18b08fcc0e11b49c791f/packages/compiler-sfc/src/script/definePropsDestructure.ts#L140-L168
        if let StmtVisitMode::Collect = self.stmt_visit_mode {
            match stmt {
                Stmt::Decl(decl) => match decl {
                    Decl::Var(var_decl) => {
                        self.collect_variable_declaration(&var_decl);
                    }

                    Decl::Class(ClassDecl { declare, ident, .. })
                    | Decl::Fn(FnDecl { declare, ident, .. }) => {
                        if *declare {
                            return;
                        }

                        self.register_local_binding(Identifier(ident.sym.to_owned(), ident.span));
                    }

                    _ => {}
                },

                // This is not covered by the official compiler
                Stmt::For(for_stmt) => {
                    if let Some(VarDeclOrExpr::VarDecl(ref var_decl)) = for_stmt.init {
                        self.collect_variable_declaration(&var_decl);
                    }
                }
                // This is covered
                Stmt::ForIn(ForInStmt { left, .. }) | Stmt::ForOf(ForOfStmt { left, .. }) => {
                    if let ForHead::VarDecl(var_decl) = left {
                        self.collect_variable_declaration(&var_decl);
                    }
                }

                Stmt::Labeled(labeled_stmt) => {
                    if let Stmt::Decl(Decl::Var(ref var_decl)) = labeled_stmt.body.as_ref() {
                        self.collect_variable_declaration(var_decl);
                    }
                }

                _ => {}
            }

            return;
        }

        // Transform mode
        stmt.visit_mut_children_with(self);
    }

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        let ident: &mut Ident = match expr {
            // TODO     if (
            //   (parent.type === 'AssignmentExpression' && id === parent.left) ||
            //   parent.type === 'UpdateExpression'
            // ) {
            //   ctx.error(`Cannot assign to destructured props as they are readonly.`, id)
            // }

            // Arrow functions need params collection
            Expr::Arrow(arrow_expr) => {
                self.push_scope();

                let mut tmp_arr = vec![];

                for param in arrow_expr.params.iter() {
                    extract_variables_from_pat(param, &mut tmp_arr, true);

                    for id in tmp_arr.drain(..) {
                        self.register_local_binding(Identifier(id.sym, id.span));
                    }
                }

                match arrow_expr.body.as_mut() {
                    BlockStmtOrExpr::BlockStmt(block_stmt) => {
                        self.collect_then_transform_stmts(&mut block_stmt.stmts);
                    }
                    BlockStmtOrExpr::Expr(expr) => {
                        expr.visit_mut_with(self);
                    }
                }

                self.pop_scope();

                return;
            }

            // Regular functions need params collection as well, but no ident transform
            Expr::Fn(fn_expr) => {
                fn_expr.function.visit_mut_with(self);
                return;
            }

            // Call expression, type arguments should not be taken into account
            Expr::Call(call_expr) => {
                self.check_usage(call_expr);

                call_expr.callee.visit_mut_with(self);
                for arg in call_expr.args.iter_mut() {
                    arg.visit_mut_with(self);
                }
                return;
            }

            // Identifier is what we need for the rest of the function
            Expr::Ident(ident) => ident,

            _ => {
                expr.visit_mut_children_with(self);
                return;
            }
        };

        // The rest concerns transforming an ident
        let span = ident.span;

        // TODO Any optimization possible?
        if self
            .excluded_ids
            .contains(&Identifier(ident.sym.to_owned(), span))
        {
            return;
        }

        if let Some(should_rewrite_with) = self.should_rewrite_with(&ident) {
            *expr = Expr::Member(MemberExpr {
                span,
                obj: Box::new(Expr::Ident(
                    PROPS_HELPER.to_owned().into_ident_spanned(span),
                )),
                prop: should_rewrite_with,
            })
        }
    }

    fn visit_mut_member_expr(&mut self, node: &mut MemberExpr) {
        if node.obj.is_ident() {
            node.obj.visit_mut_with(self)
        } else {
            node.visit_mut_children_with(self);
        }
    }

    fn visit_mut_object_lit(&mut self, object_lit: &mut ObjectLit) {
        for prop in object_lit.props.iter_mut() {
            match prop {
                PropOrSpread::Prop(ref mut prop) => {
                    // For shorthand, expand it and visit the value part
                    if let Some(shorthand) = prop.as_mut_shorthand() {
                        let prop_name = PropName::Ident(IdentName {
                            span: shorthand.span,
                            sym: shorthand.sym.to_owned(),
                        });

                        let mut value_expr = Expr::Ident(shorthand.to_owned());
                        value_expr.visit_mut_with(self);

                        *prop = Prop::KeyValue(KeyValueProp {
                            key: prop_name,
                            value: Box::new(value_expr),
                        })
                        .into();
                    } else if let Some(keyvalue) = prop.as_mut_key_value() {
                        keyvalue.value.visit_mut_with(self);
                    } else {
                        prop.visit_mut_with(self);
                    }
                }

                PropOrSpread::Spread(ref mut spread) => {
                    spread.visit_mut_with(self);
                }
            }
        }
    }

    fn visit_mut_fn_decl(&mut self, fn_decl: &mut FnDecl) {
        if fn_decl.declare {
            return;
        }

        fn_decl.function.visit_mut_with(self);
    }

    fn visit_mut_function(&mut self, function: &mut Function) {
        self.push_scope();

        let mut tmp_arr = vec![];

        for param in function.params.iter() {
            extract_variables_from_pat(&param.pat, &mut tmp_arr, true);

            for id in tmp_arr.drain(..) {
                self.register_local_binding(Identifier(id.sym, id.span));
            }
        }

        if let Some(ref mut block_stmt) = function.body {
            self.collect_then_transform_stmts(&mut block_stmt.stmts);
        }

        self.pop_scope();
    }

    // Visit the block as with respect to the variables
    fn visit_mut_block_stmt(&mut self, block_stmt: &mut BlockStmt) {
        self.push_scope();
        self.collect_then_transform_stmts(&mut block_stmt.stmts);
        self.pop_scope();
    }

    fn visit_mut_catch_clause(&mut self, catch_clause: &mut CatchClause) {
        self.push_scope();

        if let Some(Pat::Ident(ref ident)) = catch_clause.param {
            self.register_local_binding(Identifier(ident.sym.to_owned(), ident.span));
        }

        self.collect_then_transform_stmts(&mut catch_clause.body.stmts);
        self.pop_scope();
    }

    // This is a copy of `visit_mut_expr` because AssignTarget is more refined compared to Expr
    fn visit_mut_assign_target(&mut self, assign_target: &mut AssignTarget) {
        let old_is_in_assign_target = self.is_in_assign_target;
        self.is_in_assign_target = true;

        match assign_target {
            AssignTarget::Simple(simple) => match simple {
                SimpleAssignTarget::Ident(ident) => {
                    if self.should_rewrite(&ident.sym) {
                        self.errors.push(TransformError::ScriptError(ScriptError {
                            span: ident.span,
                            kind: ScriptErrorKind::DefinePropsDestructureCannotAssignToReadonly,
                        }));
                    }
                    return;
                }

                SimpleAssignTarget::Member(member) => member.visit_mut_with(self),
                SimpleAssignTarget::SuperProp(sup) => sup.visit_mut_with(self),
                SimpleAssignTarget::Paren(paren) => paren.visit_mut_with(self),
                SimpleAssignTarget::OptChain(opt_chain) => opt_chain.visit_mut_with(self),
                SimpleAssignTarget::TsAs(ts_as) => ts_as.visit_mut_with(self),
                SimpleAssignTarget::TsSatisfies(sat) => sat.visit_mut_with(self),
                SimpleAssignTarget::TsNonNull(non_null) => non_null.visit_mut_with(self),
                SimpleAssignTarget::TsTypeAssertion(type_assert) => {
                    type_assert.visit_mut_with(self)
                }
                SimpleAssignTarget::TsInstantiation(inst) => inst.visit_mut_with(self),
                SimpleAssignTarget::Invalid(_) => {}
            },

            AssignTarget::Pat(assign_target_pat) => {
                let old_is_in_destructure = self.is_in_destructure_assign;
                self.is_in_destructure_assign = true;

                match assign_target_pat {
                    AssignTargetPat::Array(arr_pat) => arr_pat.visit_mut_with(self),
                    AssignTargetPat::Object(obj_pat) => obj_pat.visit_mut_with(self),
                    AssignTargetPat::Invalid(_) => {}
                };

                self.is_in_destructure_assign = old_is_in_destructure;
            }
        }

        self.is_in_assign_target = old_is_in_assign_target;
    }

    fn visit_mut_pat(&mut self, n: &mut Pat) {
        if !self.is_in_destructure_assign {
            n.visit_mut_children_with(self);
            return;
        };

        match n {
            Pat::Ident(ident) => {
                if let Some(should_rewrite_with) = self.should_rewrite_with(&ident) {
                    *n = Pat::Expr(Box::new(Expr::Member(MemberExpr {
                        span: DUMMY_SP,
                        obj: Box::new(Expr::Ident(PROPS_HELPER.to_owned().into_ident())),
                        prop: should_rewrite_with,
                    })));
                }
                return;
            }

            Pat::Array(arr_pat) => arr_pat.visit_mut_with(self),
            Pat::Rest(rest_pat) => rest_pat.arg.visit_mut_with(self),
            Pat::Object(obj_pat) => obj_pat.visit_mut_with(self),
            Pat::Assign(assign_pat) => assign_pat.visit_mut_with(self),
            Pat::Expr(expr) => expr.visit_mut_with(self),
            Pat::Invalid(_) => {}
        }
    }

    fn visit_mut_array_pat(&mut self, arr_pat: &mut ArrayPat) {
        for maybe_pat in arr_pat.elems.iter_mut() {
            let Some(pat) = maybe_pat else {
                continue;
            };

            pat.visit_mut_with(self)
        }
    }

    fn visit_mut_object_pat(&mut self, obj_pat: &mut ObjectPat) {
        for elem in obj_pat.props.iter_mut() {
            match elem {
                // `{ x: y }`
                ObjectPatProp::KeyValue(key_value) => {
                    key_value.value.visit_mut_with(self);

                    match key_value.key {
                        PropName::Computed(ref mut computed_prop_name) => {
                            computed_prop_name.expr.visit_mut_with(self);
                        }

                        PropName::Ident(_)
                        | PropName::Str(_)
                        | PropName::Num(_)
                        | PropName::BigInt(_) => {}
                    }
                }

                ObjectPatProp::Assign(assign) => {
                    match assign.value {
                        // `{ x = y }`
                        Some(ref mut v) => v.visit_mut_with(self),

                        // If shorthand `{ x }`, expand when not a local variable
                        None => {
                            let symbol = &assign.key.sym;

                            if self.should_rewrite(symbol) {
                                let mut value = Box::new(Pat::Ident(assign.key.to_owned()));
                                value.visit_mut_with(self);
                                *elem = ObjectPatProp::KeyValue(KeyValuePatProp {
                                    key: PropName::Ident(assign.key.id.to_owned().into()),
                                    value,
                                })
                            }
                        }
                    }
                }

                // The official compiler seems to ignore this one
                ObjectPatProp::Rest(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        script::setup::macros::collect_macros,
        test_utils::{to_str, ts_module},
    };

    use super::*;

    #[test]
    fn it_works_basic_usage() {
        let mut ctx = new_ctx();
        let mut errors = vec![];

        let mut module = ts_module(
            r"
            const { msg } = defineProps<{ msg: string }>()
            console.log(msg)
            ",
        );

        collect_macros(&mut ctx, &module, &mut errors);

        transform_destructured_props(&mut ctx, &mut vec![], &mut module.body, &mut errors);

        let compiled = to_str(&module);
        assert!(!compiled.contains("const {"));
        assert!(compiled.contains("console.log(__props.msg)"));
    }

    fn new_ctx() -> TypeResolveContext {
        let mut ctx = TypeResolveContext::anonymous();
        ctx.props_destructure = PropsDestructureConfig::True;
        return ctx;
    }
}
