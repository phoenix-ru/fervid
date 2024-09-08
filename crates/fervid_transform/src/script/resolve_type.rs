//! Adapted from https://github.com/vuejs/core/blob/main/packages/compiler-sfc/src/script/resolveType.ts

use std::rc::Rc;

use fervid_core::{fervid_atom, FervidAtom, SfcScriptBlock};
use flagset::FlagSet;
use fxhash::FxHashMap as HashMap;
use itertools::Itertools;
use phf::{phf_set, Set};
use strum_macros::{AsRefStr, EnumString, IntoStaticStr};
use swc_core::{
    common::{pass::Either, Span, Spanned, DUMMY_SP},
    ecma::ast::{
        BinExpr, BinaryOp, Class, ClassDecl, Decl, DefaultDecl, ExportSpecifier, Expr, FnDecl,
        Function, Ident, Lit, ModuleDecl, ModuleExportName, ModuleItem, Pat, Stmt, Tpl,
        TsCallSignatureDecl, TsEntityName, TsEnumDecl, TsExprWithTypeArgs, TsFnOrConstructorType,
        TsFnParam, TsFnType, TsGetterSignature, TsIndexedAccessType, TsInterfaceDecl,
        TsIntersectionType, TsKeywordType, TsKeywordTypeKind, TsLit, TsLitType, TsMappedType,
        TsModuleName, TsNamespaceBody, TsQualifiedName, TsTplLitType, TsType, TsTypeAnn,
        TsTypeElement, TsTypeLit, TsTypeOperatorOp, TsTypeQueryExpr, TsTypeRef,
        TsUnionOrIntersectionType, TsUnionType,
    },
};

use crate::{
    error::{ScriptError, ScriptErrorKind},
    ImportBinding, ScopeTypeNode, TransformSfcContext, TypeOrDecl, TypeScope,
};

static SUPPORTED_BUILTINS_SET: Set<&'static str> = phf_set! {
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
};

pub type ResolutionResult<T> = Result<T, ScriptError>;

#[derive(Default)]
pub struct ResolvedElements {
    pub props: HashMap<FervidAtom, TsTypeElement>,
    pub calls: Vec<Either<TsFnType, TsCallSignatureDecl>>,
}

pub type TypeResolveContext = TransformSfcContext;

enum MergeElementsAs {
    Union,
    Intersection,
}

/// Resolve arbitrary type node to a list of type elements that can be then
/// mapped to runtime props or emits.
pub fn resolve_type_elements(
    ctx: &mut TypeResolveContext,
    ts_type: &TsType,
) -> ResolutionResult<ResolvedElements> {
    // No cache present
    let scope = ctx.scope.clone();
    return resolve_type_elements_impl_type(ctx, ts_type, &scope.borrow());
}

fn resolve_type_elements_impl_type(
    ctx: &mut TypeResolveContext,
    ts_type: &TsType,
    scope: &TypeScope,
) -> ResolutionResult<ResolvedElements> {
    // TODO Implementing a check for `@vue-ignore` requires access to comments
    // if (
    //   node.leadingComments &&
    //   node.leadingComments.some(c => c.value.includes('@vue-ignore'))
    // ) {
    //   return { props: {} }
    // }

    match ts_type {
        TsType::TsTypeLit(type_lit) => type_elements_to_map(&type_lit.members),
        TsType::TsParenthesizedType(paren) => {
            resolve_type_elements_impl_type(ctx, &paren.type_ann, scope)
        }
        TsType::TsFnOrConstructorType(fn_or_constructor) => match fn_or_constructor {
            TsFnOrConstructorType::TsFnType(fn_type) => Ok(ResolvedElements {
                props: Default::default(),
                calls: vec![Either::Left(fn_type.to_owned())],
            }),
            TsFnOrConstructorType::TsConstructorType(t) => {
                Err(error(ScriptErrorKind::ResolveTypeUnsupported, t.span))
            }
        },

        // Union
        TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(union_type)) => {
            let mut resolved_elements =
                Vec::<ResolvedElements>::with_capacity(union_type.types.len());
            for t in union_type.types.iter() {
                match resolve_type_elements_impl_type(ctx, t, scope) {
                    Ok(v) => {
                        resolved_elements.push(v);
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(merge_elements(resolved_elements, MergeElementsAs::Union))
        }

        // Intersection
        TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsIntersectionType(
            intersection_type,
        )) => {
            let mut resolved_elements =
                Vec::<ResolvedElements>::with_capacity(intersection_type.types.len());

            for t in intersection_type.types.iter() {
                match resolve_type_elements_impl_type(ctx, t, scope) {
                    Ok(v) => {
                        resolved_elements.push(v);
                    }
                    Err(e) => return Err(e),
                }
            }

            Ok(merge_elements(
                resolved_elements,
                MergeElementsAs::Intersection,
            ))
        }

        TsType::TsMappedType(mapped_type) => resolve_mapped_type(ctx, mapped_type, scope),

        TsType::TsIndexedAccessType(indexed_access_type) => {
            let types = resolve_index_type(ctx, indexed_access_type, scope)?;
            let mut resolved_elements = Vec::with_capacity(types.len());
            for t in types.iter() {
                let resolved = resolve_type_elements_impl_type(ctx, &t, scope)?;
                resolved_elements.push(resolved);
            }

            Ok(merge_elements(resolved_elements, MergeElementsAs::Union))
        }

        TsType::TsTypeRef(type_ref) => resolve_type_elements_impl_type_ref_or_expr_with_type_args(
            ctx,
            TypeRefOrExprWithTypeArgs::TsTypeRef(type_ref, ts_type),
            scope,
        ),

        TsType::TsImportType(import_type) => {
            if let Some(type_args) = import_type.type_args.as_ref() {
                if import_type.arg.value == "vue"
                    && matches!(import_type.qualifier.as_ref(), Some(TsEntityName::Ident(id)) if id.sym == "ExtractPropTypes")
                {
                    let Some(first_type_param) = type_args.params.first() else {
                        return Err(error(
                            ScriptErrorKind::ResolveTypeMissingTypeParam,
                            type_args.span,
                        ));
                    };

                    let resolved_elements =
                        resolve_type_elements_impl_type(ctx, &first_type_param, scope)?;

                    return resolve_extract_prop_types(ctx, resolved_elements);
                }
            }

            // TODO
            // const sourceScope = importSourceToScope(
            // ctx,
            // node.argument,
            // scope,
            // node.argument.value,
            // )
            // const resolved = resolveTypeReference(ctx, node, sourceScope)
            // if (resolved) {
            // return resolveTypeElements(ctx, resolved, resolved._ownerScope)
            // }
            // break

            Err(error(ScriptErrorKind::ResolveTypeUnsupported, import_type.span))
        }

        TsType::TsTypeQuery(type_query) => {
            if let Some(resolved) =
                resolve_type_reference(ctx, ReferenceTypes::TsType(ts_type), scope)
            {
                match &resolved.value {
                    TypeOrDecl::Type(ts_type) => {
                        resolve_type_elements_impl_type(ctx, &ts_type, scope)
                    }
                    TypeOrDecl::Decl(decl) => resolve_type_elements_impl_decl(ctx, &decl, scope),
                }
            } else {
                Err(error(
                    ScriptErrorKind::ResolveTypeUnresolvable,
                    type_query.span,
                ))
            }
        }

        // TsType::TsKeywordType(_)
        // | TsType::TsThisType(_)
        // | TsType::TsArrayType(_)
        // | TsType::TsTupleType(_)
        // | TsType::TsOptionalType(_)
        // | TsType::TsRestType(_)
        // | TsType::TsConditionalType(_)
        // | TsType::TsInferType(_)
        // | TsType::TsTypeOperator(_)
        // | TsType::TsLitType(_)
        // | TsType::TsTypePredicate(_)
        x => Err(error(ScriptErrorKind::ResolveTypeUnresolvable, x.span())),
    }
}

fn resolve_type_elements_impl_decl(
    ctx: &mut TypeResolveContext,
    decl: &Decl,
    scope: &TypeScope,
) -> ResolutionResult<ResolvedElements> {
    // TODO Implementing a check for `@vue-ignore` requires access to comments
    // if (
    //   node.leadingComments &&
    //   node.leadingComments.some(c => c.value.includes('@vue-ignore'))
    // ) {
    //   return { props: {} }
    // }

    match decl {
        Decl::TsInterface(interface) => resolve_interface_members(ctx, interface, scope),
        _ => Err(error(ScriptErrorKind::ResolveTypeUnresolvable, decl.span())),
    }
}

enum TypeRefOrExprWithTypeArgs<'t> {
    TsTypeRef(&'t TsTypeRef, &'t TsType),
    TsExprWithTypeArgs(&'t TsExprWithTypeArgs),
}

fn resolve_type_elements_impl_type_ref_or_expr_with_type_args(
    ctx: &mut TypeResolveContext,
    node: TypeRefOrExprWithTypeArgs,
    scope: &TypeScope,
) -> ResolutionResult<ResolvedElements> {
    let (reference_type, type_params, span) = match node {
        TypeRefOrExprWithTypeArgs::TsTypeRef(type_ref, ts_type) => (
            ReferenceTypes::TsType(ts_type),
            type_ref.type_params.as_ref(),
            type_ref.span,
        ),

        TypeRefOrExprWithTypeArgs::TsExprWithTypeArgs(expr_with_type_args) => (
            ReferenceTypes::TsExprWithTypeArgs(expr_with_type_args),
            expr_with_type_args.type_args.as_ref(),
            expr_with_type_args.span,
        ),
    };

    let type_name = get_reference_name(reference_type);
    let type_name_single = if type_name.len() == 1 {
        type_name.get(0)
    } else {
        None
    };

    // Condition:
    // (typeName === 'ExtractPropTypes' ||
    //   typeName === 'ExtractPublicPropTypes') &&
    //   node.typeParameters &&
    //   scope.imports[typeName]?.source === 'vue'
    match type_name_single {
        Some(type_name_single)
            if type_name_single == "ExtractPropTypes"
                || type_name_single == "ExtractPublicPropTypes" =>
        'm: {
            let Some(import) = scope.imports.get(type_name_single) else {
                break 'm;
            };

            if import.source != "vue" {
                break 'm;
            }

            let Some(ref type_params) = type_params else {
                break 'm;
            };

            let Some(first_type_param) = type_params.params.first() else {
                return Err(error(
                    ScriptErrorKind::ResolveTypeMissingTypeParam,
                    type_params.span,
                ));
            };

            let resolved_elements = resolve_type_elements_impl_type(ctx, &first_type_param, scope)?;
            return resolve_extract_prop_types(ctx, resolved_elements);
        }
        _ => {}
    }

    let resolved = resolve_type_reference(ctx, reference_type, scope);
    if let Some(resolved) = resolved {
        // TODO
        // let typeParams: Record<string, Node> | undefined
        // if (
        //     (resolved.type === 'TSTypeAliasDeclaration' ||
        //     resolved.type === 'TSInterfaceDeclaration') &&
        //     resolved.typeParameters &&
        //     node.typeParameters
        // ) {
        //     typeParams = Object.create(null)
        //     resolved.typeParameters.params.forEach((p, i) => {
        //     let param = typeParameters && typeParameters[p.name]
        //     if (!param) param = node.typeParameters!.params[i]
        //     typeParams![p.name] = param
        //     })
        // }
        // return resolveTypeElements(
        //     ctx,
        //     resolved,
        //     resolved._ownerScope,
        //     typeParams,
        // )

        // TODO `resolved._ownerScope`
        return match resolved.value {
            TypeOrDecl::Type(ref ts_type) => resolve_type_elements_impl_type(ctx, &ts_type, scope),
            TypeOrDecl::Decl(ref decl) => resolve_type_elements_impl_decl(ctx, &decl, scope),
        };
    }

    let Some(type_name_single) = type_name_single else {
        return Err(error(ScriptErrorKind::ResolveTypeUnsupported, span));
    };

    // TODO typeParameters
    // if (typeParameters && typeParameters[typeName]) {
    //     return resolveTypeElements(
    //         ctx,
    //         typeParameters[typeName],
    //         scope,
    //         typeParameters,
    //     )
    // }

    if SUPPORTED_BUILTINS_SET.contains(type_name_single) {
        return resolve_builtin(ctx, node, type_name_single, scope);
    } else if let ("ReturnType", Some(ref type_params)) =
        (type_name_single.as_str(), type_params.as_ref())
    {
        // limited support, only reference types
        let Some(first_type_param) = type_params.params.first() else {
            return Err(error(
                ScriptErrorKind::ResolveTypeMissingTypeParam,
                type_params.span,
            ));
        };

        // Inline implementation of `resolve_return_type` to avoid unnecessary clones
        'resolve_return_type: {
            let ts_type = first_type_param.as_ref();
            let mut resolved: Option<ScopeTypeNode> = None;
            if matches!(
                ts_type,
                TsType::TsTypeRef(_) | TsType::TsTypeQuery(_) | TsType::TsImportType(_)
            ) {
                resolved = resolve_type_reference(ctx, ReferenceTypes::TsType(&ts_type), scope);
            }

            let Some(resolved) = resolved else {
                break 'resolve_return_type;
            };

            let return_type = match resolved.value {
                TypeOrDecl::Type(ref ts_type) => match ts_type.as_ref() {
                    TsType::TsFnOrConstructorType(TsFnOrConstructorType::TsFnType(fn_type)) => {
                        Some(fn_type.type_ann.type_ann.as_ref())
                    }

                    _ => None,
                },

                TypeOrDecl::Decl(ref decl) => match decl.as_ref() {
                    Decl::Fn(fn_decl) => fn_decl
                        .function
                        .return_type
                        .as_ref()
                        .map(|v| v.type_ann.as_ref()),

                    _ => None,
                },
            };

            if let Some(ret) = return_type {
                return resolve_type_elements_impl_type(ctx, ret, scope);
            }
        }
    }

    Err(error(ScriptErrorKind::ResolveTypeUnsupported, span))
}

fn type_elements_to_map(elements: &Vec<TsTypeElement>) -> ResolutionResult<ResolvedElements> {
    let mut result = ResolvedElements::default();

    for ts_type_element in elements.iter() {
        macro_rules! implementation {
            ($signature: ident) => {
                // TODO No scopes supported

                let name = get_id(&$signature.key);
                if let (Some(name), false) = (name, $signature.computed) {
                    result.props.insert(name, ts_type_element.to_owned());
                } else if let Expr::Tpl(tpl) = $signature.key.as_ref() {
                    let keys = resolve_template_keys(&tpl)?;
                    for key in keys {
                        result.props.insert(key, ts_type_element.to_owned());
                    }
                } else {
                    return Err(error(
                        ScriptErrorKind::ResolveTypeUnsupportedComputedKey,
                        $signature.span,
                    ));
                }
            };
        }
        match ts_type_element {
            TsTypeElement::TsPropertySignature(ref signature) => {
                implementation!(signature);
            }
            TsTypeElement::TsMethodSignature(ref signature) => {
                implementation!(signature);
            }

            TsTypeElement::TsCallSignatureDecl(ref signature) => {
                result.calls.push(Either::Right(signature.to_owned()));
            }

            // TsTypeElement::TsConstructSignatureDecl(_) => {},
            // TsTypeElement::TsGetterSignature(_) => {},
            // TsTypeElement::TsSetterSignature(_) => {},
            // TsTypeElement::TsIndexSignature(_) => {},
            _ => {}
        }
    }

    Ok(result)
}

fn merge_elements(
    mut elements: Vec<ResolvedElements>,
    merge_as: MergeElementsAs,
) -> ResolvedElements {
    if elements.len() == 1 {
        return elements.pop().unwrap();
    }

    let mut result = ResolvedElements::default();

    for ResolvedElements { props, mut calls } in elements {
        // Add props
        for (key, new_value) in props {
            match result.props.get(&key) {
                None => {
                    result.props.insert(key, new_value);
                }

                Some(existing_value) => {
                    let optional = match existing_value {
                        TsTypeElement::TsPropertySignature(s) => s.optional,
                        TsTypeElement::TsGetterSignature(s) => s.optional,
                        TsTypeElement::TsMethodSignature(s) => s.optional,
                        _ => false,
                    };

                    macro_rules! implementation {
                        ($signature: ident) => {
                            // TODO Validate. Probably just pushing to existing is better?
                            let types: Vec<Box<TsType>> = vec![
                                // existing
                                Box::new(TsType::TsTypeLit(TsTypeLit {
                                    span: DUMMY_SP,
                                    members: vec![existing_value.to_owned()],
                                })),
                                // new
                                Box::new(TsType::TsTypeLit(TsTypeLit {
                                    span: DUMMY_SP,
                                    members: vec![new_value.to_owned()],
                                })),
                            ];

                            let union_or_intersection = match merge_as {
                                MergeElementsAs::Union => {
                                    TsUnionOrIntersectionType::TsUnionType(TsUnionType {
                                        span: DUMMY_SP,
                                        types,
                                    })
                                }
                                MergeElementsAs::Intersection => {
                                    TsUnionOrIntersectionType::TsIntersectionType(
                                        TsIntersectionType {
                                            span: DUMMY_SP,
                                            types,
                                        },
                                    )
                                }
                            };

                            let property = create_property(
                                $signature.key.to_owned(),
                                Box::new(TsType::TsUnionOrIntersectionType(union_or_intersection)),
                                $signature.optional || optional,
                            );

                            result
                                .props
                                .insert(key, TsTypeElement::TsGetterSignature(property));
                        };
                    }

                    match new_value {
                        TsTypeElement::TsMethodSignature(ref signature) => {
                            implementation!(signature);
                        }
                        TsTypeElement::TsPropertySignature(ref signature) => {
                            implementation!(signature);
                        }

                        // TsTypeElement::TsCallSignatureDecl(_) => {},
                        // TsTypeElement::TsGetterSignature(_) => {},
                        // TsTypeElement::TsConstructSignatureDecl(_) => {},
                        // TsTypeElement::TsSetterSignature(_) => {},
                        // TsTypeElement::TsIndexSignature(_) => {},
                        _ => {
                            // ??
                        }
                    }
                }
            }
        }

        // Add calls
        result.calls.append(&mut calls);
    }

    result
}

fn resolve_interface_members(
    ctx: &mut TypeResolveContext,
    interface_decl: &TsInterfaceDecl,
    scope: &TypeScope, // TODO Type parameters
) -> ResolutionResult<ResolvedElements> {
    let mut base = type_elements_to_map(&interface_decl.body.body)?;

    for ext in interface_decl.extends.iter() {
        let Ok(mut resolved) = resolve_type_elements_impl_type_ref_or_expr_with_type_args(
            ctx,
            TypeRefOrExprWithTypeArgs::TsExprWithTypeArgs(ext),
            scope,
        ) else {
            return Err(error(ScriptErrorKind::ResolveTypeExtendsBaseType, ext.span));
        };

        for (key, value) in resolved.props {
            if !base.props.contains_key(&key) {
                base.props.insert(key, value);
            }
        }

        base.calls.append(&mut resolved.calls);
    }

    Ok(base)
}

fn resolve_mapped_type(
    ctx: &mut TypeResolveContext,
    mapped_type: &TsMappedType,
    scope: &TypeScope,
) -> ResolutionResult<ResolvedElements> {
    let mut result = ResolvedElements::default();

    let keys = if let Some(ref name_type) = mapped_type.name_type {
        // TODO Scope
        resolve_string_type(ctx, &name_type, scope)?
    } else if let Some(ref constraint) = mapped_type.type_param.constraint {
        resolve_string_type(ctx, &constraint, scope)?
    } else {
        // Constraint must be present, otherwise we can't resolve
        return Err(error(
            ScriptErrorKind::ResolveTypeUnresolvable,
            mapped_type.type_param.span,
        ));
    };

    let Some(ref type_ann) = mapped_type.type_ann else {
        // Same for type annotation - cannot continue without it
        return Err(error(
            ScriptErrorKind::ResolveTypeUnresolvable,
            mapped_type.span,
        ));
    };

    for key in keys {
        let property = create_property(
            Box::new(Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: key.to_owned(),
                optional: false,
            })),
            type_ann.to_owned(),
            mapped_type.optional.is_some(),
        );

        result
            .props
            .insert(key, TsTypeElement::TsGetterSignature(property));
    }

    Ok(result)
}

fn resolve_index_type(
    ctx: &mut TypeResolveContext,
    index_type: &TsIndexedAccessType,
    scope: &TypeScope,
) -> ResolutionResult<Vec<Box<TsType>>> {
    let TsIndexedAccessType {
        obj_type,
        index_type,
        ..
    } = index_type;

    if let TsType::TsLitType(TsLitType {
        lit: TsLit::Number(_),
        ..
    }) = index_type.as_ref()
    {
        return resolve_array_element_type(ctx, &obj_type, scope);
    }

    let resolved = resolve_type_elements_impl_type(ctx, &obj_type, scope)?;
    let mut props = resolved.props;
    let mut types = Vec::<Box<TsType>>::new();

    macro_rules! implementation {
        ($value: ident) => {
            let target_type = match $value {
                TsTypeElement::TsPropertySignature(ref s) => &s.type_ann,
                TsTypeElement::TsGetterSignature(ref s) => &s.type_ann,
                TsTypeElement::TsMethodSignature(ref s) => &s.type_ann,
                _ => continue,
            };

            if let Some(ref type_ann) = target_type {
                types.push(type_ann.type_ann.to_owned());
            }
        };
    }

    if let TsType::TsKeywordType(TsKeywordType {
        kind: TsKeywordTypeKind::TsStringKeyword,
        ..
    }) = index_type.as_ref()
    {
        // Values of the map
        for (_key, value) in props.drain() {
            implementation!(value);
        }
    } else {
        // Values of the string type
        for key in resolve_string_type(ctx, &index_type, scope)? {
            let Some(value) = props.remove(&key) else {
                continue;
            };

            implementation!(value);
        }
    };

    Ok(types)
}

fn resolve_array_element_type(
    ctx: &mut TypeResolveContext,
    array_element_type: &TsType,
    scope: &TypeScope,
) -> ResolutionResult<Vec<Box<TsType>>> {
    match array_element_type {
        // type[]
        TsType::TsArrayType(array_type) => Ok(vec![array_type.elem_type.to_owned()]),

        // tuple
        TsType::TsTupleType(tuple_type) => Ok(tuple_type
            .elem_types
            .iter()
            .map(|t| t.ty.to_owned())
            .collect_vec()),

        TsType::TsTypeRef(ref type_ref) => {
            let ref_name = get_reference_name_from_entity(&type_ref.type_name);
            let ref_name = if ref_name.len() == 1 {
                &ref_name[0]
            } else {
                ""
            };

            // Array<Type>
            if let ("Array", Some(type_params)) = (ref_name, type_ref.type_params.as_ref()) {
                return Ok(type_params
                    .params
                    .iter()
                    .map(|it| it.to_owned())
                    .collect_vec());
            }

            // Reference
            if let Some(resolved) =
                resolve_type_reference(ctx, ReferenceTypes::TsType(array_element_type), scope)
            {
                if let TypeOrDecl::Type(ts_type) = &resolved.value {
                    return resolve_array_element_type(ctx, &ts_type, scope);
                }
            };

            Err(error(
                ScriptErrorKind::ResolveTypeElementType,
                type_ref.span,
            ))
        }

        x => Err(error(ScriptErrorKind::ResolveTypeElementType, x.span())),
    }
}

fn get_reference_name(ts_type: ReferenceTypes) -> Vec<FervidAtom> {
    match ts_type {
        ReferenceTypes::TsExprWithTypeArgs(ts_expr_with_type_args) => {
            let expr = &ts_expr_with_type_args.expr;
            if let Expr::Ident(ref ident) = expr.as_ref() {
                return vec![ident.sym.to_owned()];
            }
        }

        ReferenceTypes::TsType(ts_type) => {
            let reference = match ts_type {
                TsType::TsTypeRef(type_ref) => Some(&type_ref.type_name),

                TsType::TsImportType(import_type) => import_type.qualifier.as_ref(),

                TsType::TsTypeQuery(type_query) => match type_query.expr_name {
                    TsTypeQueryExpr::TsEntityName(ref entity_name) => Some(entity_name),
                    TsTypeQueryExpr::Import(ref import_type) => import_type.qualifier.as_ref(),
                },

                _ => None,
            };

            if let Some(entity_name) = reference {
                return get_reference_name_from_entity(entity_name);
            }
        }
    }

    vec![fervid_atom!("default")]
}

fn get_reference_name_from_entity(ts_entity_name: &TsEntityName) -> Vec<FervidAtom> {
    match ts_entity_name {
        TsEntityName::Ident(ident) => vec![ident.sym.to_owned()],
        TsEntityName::TsQualifiedName(qualified_name) => qualified_name_to_path(&qualified_name),
    }
}

fn qualified_name_to_path(qual_name: &TsQualifiedName) -> Vec<FervidAtom> {
    let mut idents = vec![&qual_name.right];
    let mut next_entity = &qual_name.left;
    let mut has_next = true;
    while has_next {
        match next_entity {
            TsEntityName::TsQualifiedName(next_qual_name) => {
                idents.push(&next_qual_name.right);
                next_entity = &next_qual_name.left;
                has_next = true;
            }
            TsEntityName::Ident(ref ident) => {
                idents.push(ident);
                has_next = false;
            }
        }
    }

    idents
        .into_iter()
        .rev()
        .map(|ident| ident.sym.to_owned())
        .collect_vec()
}

fn resolve_global_scope(_ctx: &mut TypeResolveContext) -> Result<Option<Vec<TypeScope>>, ()> {
    // function resolveGlobalScope(ctx: TypeResolveContext): TypeScope[] | undefined {
    //     if (ctx.options.globalTypeFiles) {
    //       const fs = resolveFS(ctx)
    //       if (!fs) {
    //         throw new Error('[vue/compiler-sfc] globalTypeFiles requires fs access.')
    //       }
    //       return ctx.options.globalTypeFiles.map(file =>
    //         fileToScope(ctx, normalizePath(file), true),
    //       )
    //     }
    //   }

    // TODO: Implement when FS access is ready
    Ok(None)
}

fn resolve_type_from_import<'t>(
    _ctx: &mut TypeResolveContext,
    _ts_type: ReferenceTypes<'t>,
    _name: &str,
    _scope: &TypeScope,
) -> Option<ScopeTypeNode> {
    // const { source, imported } = scope.imports[name]
    // const sourceScope = importSourceToScope(ctx, node, scope, source)
    // return resolveTypeReference(ctx, node, sourceScope, imported, true)

    // TODO: Implement when FS access is ready
    None
}

fn resolve_template_keys(tpl: &Tpl) -> ResolutionResult<Vec<FervidAtom>> {
    struct StackItem {
        expr_idx: usize,
        quasi_idx: usize,
        prefix: String,
    }

    let mut results = Vec::new();
    let mut stack = vec![StackItem {
        expr_idx: 0,
        quasi_idx: 0,
        prefix: "".to_string(),
    }];

    while let Some(StackItem {
        expr_idx,
        quasi_idx,
        prefix,
    }) = stack.pop()
    {
        let q = tpl.quasis.get(quasi_idx);
        let leading = q.map_or("", |v| &v.raw);

        if expr_idx >= tpl.exprs.len() {
            results.push(FervidAtom::from(format!("{prefix}{leading}")));
            continue;
        }

        let resolved = resolve_string_type_expr(&tpl.exprs[expr_idx])?;

        for r in resolved {
            stack.push(StackItem {
                expr_idx: expr_idx + 1,
                quasi_idx: quasi_idx + 1,
                prefix: format!("{prefix}{leading}{r}"),
            });
        }
    }

    Ok(results)
}

fn resolve_template_keys_ts(
    ctx: &mut TypeResolveContext,
    tpl: &TsTplLitType,
    scope: &TypeScope,
) -> ResolutionResult<Vec<FervidAtom>> {
    struct StackItem {
        expr_idx: usize,
        quasi_idx: usize,
        prefix: String,
    }

    let mut results = Vec::new();
    let mut stack = vec![StackItem {
        expr_idx: 0,
        quasi_idx: 0,
        prefix: "".to_string(),
    }];

    while let Some(StackItem {
        expr_idx,
        quasi_idx,
        prefix,
    }) = stack.pop()
    {
        let q = tpl.quasis.get(quasi_idx);
        let leading = q.map_or("", |v| &v.raw);

        if expr_idx >= tpl.types.len() {
            results.push(FervidAtom::from(format!("{prefix}{leading}")));
            continue;
        }

        let resolved = resolve_string_type(ctx, &tpl.types[expr_idx], scope)?;

        for r in resolved {
            stack.push(StackItem {
                expr_idx: expr_idx + 1,
                quasi_idx: quasi_idx + 1,
                prefix: format!("{prefix}{leading}{r}"),
            });
        }
    }

    Ok(results)
}

fn resolve_string_type(
    ctx: &mut TypeResolveContext,
    ts_type: &TsType,
    scope: &TypeScope,
) -> ResolutionResult<Vec<FervidAtom>> {
    match ts_type {
        TsType::TsLitType(lit_type) => match lit_type.lit {
            TsLit::Str(ref s) => Ok(vec![s.value.to_owned()]),

            TsLit::Tpl(ref tpl) => resolve_template_keys_ts(ctx, tpl, scope),

            ref x => Err(error(
                ScriptErrorKind::ResolveTypeUnsupportedIndexType,
                x.span(),
            )),
        },

        TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(union_type)) => {
            let mut result = Vec::new();
            for typ in union_type.types.iter() {
                result.append(&mut resolve_string_type(ctx, &typ, scope)?);
            }
            Ok(result)
        }

        TsType::TsTypeRef(type_ref) => {
            let resolved = resolve_type_reference(ctx, ReferenceTypes::TsType(ts_type), scope);
            if let Some(resolved) = resolved {
                // Only type supported in the call below
                let TypeOrDecl::Type(ref ts_type) = resolved.value else {
                    return Err(error(
                        ScriptErrorKind::ResolveTypeUnresolvableIndexType,
                        type_ref.span,
                    ));
                };

                return resolve_string_type(ctx, &ts_type, scope);
            }

            let TsEntityName::Ident(ref type_name_ident) = type_ref.type_name else {
                return Err(error(
                    ScriptErrorKind::ResolveTypeUnsupportedIndexType,
                    type_ref.type_name.span(),
                ));
            };

            let Some(ref type_params) = type_ref.type_params else {
                return Err(error(
                    ScriptErrorKind::ResolveTypeMissingTypeParams,
                    type_ref.span,
                ));
            };

            let mut get_param = |idx: usize| {
                let param = type_params.params.get(idx);
                match param {
                    Some(p) => resolve_string_type(ctx, &p, scope),
                    None => Err(error(
                        ScriptErrorKind::ResolveTypeMissingTypeParam,
                        type_params.span,
                    )),
                }
            };

            match type_name_ident.sym.as_str() {
                "Extract" => get_param(1),
                "Exclude" => {
                    let mut all = get_param(0)?;
                    let excluded = get_param(1)?;
                    all.retain(|it| !excluded.contains(it));
                    Ok(all)
                }
                "Uppercase" => {
                    let mut result = get_param(0)?;
                    result
                        .iter_mut()
                        .for_each(|it| *it = FervidAtom::from(it.to_uppercase()));
                    Ok(result)
                }
                "Lowercase" => {
                    let mut result = get_param(0)?;
                    result
                        .iter_mut()
                        .for_each(|it| *it = FervidAtom::from(it.to_lowercase()));
                    Ok(result)
                }
                "Capitalize" => {
                    let mut result = get_param(0)?;
                    capitalize_or_uncapitalize_atoms(&mut result, true);
                    Ok(result)
                }
                "Uncapitalize" => {
                    let mut result = get_param(0)?;
                    capitalize_or_uncapitalize_atoms(&mut result, false);
                    Ok(result)
                }
                _ => Err(error(
                    ScriptErrorKind::ResolveTypeUnsupportedIndexType,
                    type_name_ident.span,
                )),
            }
        }

        x => Err(error(
            ScriptErrorKind::ResolveTypeUnresolvableIndexType,
            x.span(),
        )),
    }
}

fn resolve_string_type_expr(expr: &Expr) -> ResolutionResult<Vec<FervidAtom>> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Ok(vec![s.value.to_owned()]),

        // Unions and intersections are not valid members here (sad).
        // So we have to work around using BinExpr (e.g. `'foo' | 'bar'`)
        Expr::Bin(BinExpr {
            op: BinaryOp::BitOr,
            left,
            right,
            ..
        }) => {
            let mut left = resolve_string_type_expr(&left)?;
            let mut right = resolve_string_type_expr(&right)?;
            left.append(&mut right);
            Ok(left)
        }

        Expr::Tpl(tpl) => resolve_template_keys(tpl),

        // Type references are not supported (since Expr is not a proper TS type)
        x => Err(error(
            ScriptErrorKind::ResolveTypeUnresolvableIndexType,
            x.span(),
        )),
    }
}

fn resolve_builtin(
    ctx: &mut TypeResolveContext,
    node: TypeRefOrExprWithTypeArgs,
    name: &str,
    scope: &TypeScope,
) -> ResolutionResult<ResolvedElements> {
    let (type_params, span) = match node {
        TypeRefOrExprWithTypeArgs::TsTypeRef(type_ref, _) => {
            (type_ref.type_params.as_ref(), type_ref.span)
        }
        TypeRefOrExprWithTypeArgs::TsExprWithTypeArgs(expr_with_type_args) => (
            expr_with_type_args.type_args.as_ref(),
            expr_with_type_args.span,
        ),
    };

    let Some(ref type_params) = type_params else {
        return Err(error(ScriptErrorKind::ResolveTypeMissingTypeParams, span));
    };

    let Some(first_type_param) = type_params.params.first() else {
        return Err(error(
            ScriptErrorKind::ResolveTypeMissingTypeParam,
            type_params.span,
        ));
    };

    let mut t = resolve_type_elements(ctx, &first_type_param)?;

    match name {
        "Partial" | "Required" => {
            let is_optional = name == "Partial";

            for prop in t.props.values_mut() {
                match prop {
                    TsTypeElement::TsPropertySignature(s) => s.optional = is_optional,
                    TsTypeElement::TsGetterSignature(s) => s.optional = is_optional,
                    TsTypeElement::TsSetterSignature(s) => s.optional = is_optional,
                    TsTypeElement::TsMethodSignature(s) => s.optional = is_optional,
                    _ => {}
                }
            }

            Ok(t)
        }

        "Readonly" => Ok(t),

        "Pick" | "Omit" => {
            let should_stay = name == "Pick";

            let Some(second_type_param) = type_params.params.get(1) else {
                return Err(error(
                    ScriptErrorKind::ResolveTypeMissingTypeParam,
                    type_params.span,
                ));
            };

            let picked_or_omitted = resolve_string_type(ctx, &second_type_param, scope)?;

            // Q: is rebuilding a map faster than doing `retain`?
            // `retain` docs suggest `O(capacity)`, not `O(n)`
            t.props.retain(|k, _v| {
                let has = picked_or_omitted.contains(k);
                has == should_stay
            });

            Ok(t)
        }

        // This should not be possible
        _ => unreachable!("Unknown builtin passed to resolve_builtin: {name}"),
    }
}

fn find_static_property_type<'t>(ts_type: &'t TsTypeLit, key: &str) -> Option<&'t TsType> {
    ts_type.members.iter().find_map(|m| {
        let TsTypeElement::TsPropertySignature(s) = m else {
            return None;
        };

        if let (false, Some(k), Some(type_ann)) = (s.computed, get_id(&s.key), s.type_ann.as_ref())
        {
            if k == key {
                return Some(type_ann.type_ann.as_ref());
            }
        }

        None
    })
}

pub fn resolve_union_type(
    ctx: &mut TypeResolveContext,
    ts_type: &TsType,
    scope: &TypeScope,
) -> Vec<TypeOrDecl> {
    let mut result = Vec::new();
    resolve_union_type_impl(ctx, ts_type, scope, &mut result);
    result
}

fn resolve_union_type_impl(
    ctx: &mut TypeResolveContext,
    ts_type: &TsType,
    scope: &TypeScope,
    out: &mut Vec<TypeOrDecl>,
) {
    macro_rules! union_or_intersection {
        ($ts_type: expr, $ts_type_else: expr) => {
            if let TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(
                union_type,
            )) = $ts_type
            {
                for union_type_child in union_type.types.iter() {
                    resolve_union_type_impl(ctx, &union_type_child, scope, out);
                }
            } else {
                out.push(TypeOrDecl::Type($ts_type_else));
            }
        };
    }

    // Try resolving a type reference
    if let TsType::TsTypeRef(_) = ts_type {
        let resolved = resolve_type_reference(ctx, ReferenceTypes::TsType(&ts_type), scope);

        if let Some(resolved) = resolved {
            let ts_type = match resolved.value {
                TypeOrDecl::Type(t) => {
                    // Use resolved type as target
                    t
                }
                TypeOrDecl::Decl(ref decl) => {
                    out.push(TypeOrDecl::Decl(decl.clone()));
                    return;
                }
            };

            // We duplicate the condition because of borrow-checks on Rc<TsType> vs &TsType
            union_or_intersection!(ts_type.as_ref(), ts_type);
            return;
        }
    }

    union_or_intersection!(ts_type, Rc::from(ts_type.to_owned()));
}

#[derive(Clone, Copy)]
enum ReferenceTypes<'t> {
    TsType(&'t TsType),
    TsExprWithTypeArgs(&'t TsExprWithTypeArgs),
}

fn resolve_type_reference<'t>(
    ctx: &mut TypeResolveContext,
    ts_type: ReferenceTypes<'t>,
    scope: &'t TypeScope,
) -> Option<ScopeTypeNode> {
    // TODO No type resolution is implemented yet
    // TODO Implementing this requires scopes
    // TODO It also requires a FS layer to work the same way as in official compiler

    // TODO No caching is supported
    //     const canCache = !scope?.isGenericScope
    //     if (canCache && node._resolvedReference) {
    //       return node._resolvedReference
    //     }

    let name = get_reference_name(ts_type);
    inner_resolve_type_reference(ctx, ts_type, scope, &name, false)
}

fn inner_resolve_type_reference<'t>(
    ctx: &mut TypeResolveContext,
    ts_type: ReferenceTypes<'t>,
    scope: &'t TypeScope,
    name: &[FervidAtom],
    only_exported: bool,
) -> Option<ScopeTypeNode> {
    let name_single = if name.len() == 1 {
        Some(&name[0])
    } else if name.len() > 1 {
        None
    } else {
        return None;
    };

    if let Some(name_single) = name_single {
        if let Some(_) = scope.imports.get(name_single) {
            return resolve_type_from_import(ctx, ts_type, &name_single, scope);
        };

        let lookup_source = match ts_type {
            ReferenceTypes::TsType(TsType::TsTypeQuery(_)) if only_exported => {
                &scope.exported_declares
            }
            ReferenceTypes::TsType(TsType::TsTypeQuery(_)) => &scope.declares,
            _ if only_exported => &scope.exported_types,
            _ => &scope.types,
        };

        if let Some(found) = lookup_source.get(name_single) {
            return Some(found.to_owned());
        }

        // fallback to global
        let global_scopes = resolve_global_scope(ctx);
        if let Ok(Some(global_scopes)) = global_scopes {
            for s in global_scopes {
                let src = if matches!(ts_type, ReferenceTypes::TsType(TsType::TsTypeQuery(_))) {
                    &s.declares
                } else {
                    &s.types
                };
                if let Some(found) = src.get(name_single) {
                    ctx.deps.insert(s.filename.to_owned());
                    return Some(found.to_owned());
                }
            }
        }

        // Not found
        return None;
    }

    let ns = inner_resolve_type_reference(ctx, ts_type, scope, &name[0..1], only_exported);
    if let Some(_ns) = ns {
        // TODO This is pretty much impossible to cover
        //   1: TSModuleDeclaration is not a part of TsType;
        //   2: It's not possible to attach meta-information;
        //
        // if (ns.type !== 'TSModuleDeclaration') {
        //   // namespace merged with other types, attached as _ns
        //   ns = ns._ns
        // }
        //         if (ns) {
        //           const childScope = moduleDeclToScope(ctx, ns, ns._ownerScope || scope)
        //           return innerResolveTypeReference(
        //             ctx,
        //             childScope,
        //             name.length > 2 ? name.slice(1) : name[name.length - 1],
        //             node,
        //             !ns.declare,
        //           )
        //         }
    }

    None
}

pub fn record_types(
    _ctx: &mut TransformSfcContext,
    script_setup: Option<&SfcScriptBlock>,
    script_options: Option<&SfcScriptBlock>,
    scope: &mut TypeScope,
    as_global: bool,
) {
    let TypeScope {
        imports,
        types,
        declares,
        exported_types,
        exported_declares,
        ..
    } = scope;

    let (body, setup_offset) = match (script_setup, script_options) {
        (None, None) => return,
        (None, Some(o)) => (Either::Left(o.content.body.iter()), None),
        (Some(s), None) => (Either::Left(s.content.body.iter()), Some(0)),
        (Some(s), Some(o)) => (
            Either::Right(o.content.body.iter().chain(s.content.body.iter())),
            Some(o.content.body.len()),
        ),
    };

    // Ambient means no imports or exports present
    let is_ambient = as_global && !body.clone().any(|s| matches!(s, ModuleItem::ModuleDecl(_)));

    // We clone the iterator several times so that it can be used again.
    // This has no impact on perf.
    for module_item in body.clone() {
        if as_global {
            if is_ambient {
                if is_declare(module_item) {}
            } else if let ModuleItem::Stmt(Stmt::Decl(Decl::TsModule(module))) = module_item {
                if !module.global {
                    break;
                }

                let Some(TsNamespaceBody::TsModuleBlock(ref module)) = module.body else {
                    break;
                };

                for s in module.body.iter() {
                    record_type_module_item(s, types, declares, None);
                }
            }
        } else {
            record_type_module_item(module_item, types, declares, None);
        }
    }

    if !as_global {
        for (idx, stmt) in body.enumerate() {
            match stmt {
                ModuleItem::ModuleDecl(module_decl) => match module_decl {
                    ModuleDecl::ExportDecl(decl) => {
                        record_type_decl(&decl.decl, types, declares, None);
                        record_type_decl(&decl.decl, exported_types, exported_declares, None);
                    }

                    ModuleDecl::ExportNamed(named) => {
                        /// Gets the atom from ident or string
                        fn get_id(n: &ModuleExportName) -> FervidAtom {
                            match n {
                                ModuleExportName::Ident(i) => i.sym.to_owned(),
                                ModuleExportName::Str(s) => s.value.to_owned(),
                            }
                        }

                        for spec in named.specifiers.iter() {
                            let ExportSpecifier::Named(spec) = spec else {
                                continue;
                            };

                            let local = get_id(&spec.orig);
                            let exported = spec
                                .exported
                                .as_ref()
                                .map(get_id)
                                .unwrap_or_else(|| local.to_owned());

                            if let Some(ref source) = named.src {
                                let is_from_setup = setup_offset.is_some_and(|v| idx >= v);

                                // re-export, register an import + export as a type reference
                                imports.insert(
                                    exported.to_owned(),
                                    ImportBinding {
                                        source: source.value.to_owned(),
                                        imported: local.to_owned(),
                                        local: local.to_owned(),
                                        is_from_setup,
                                    },
                                );

                                // We can use IDs for scopes (lookup by ID, store ID on the scope level and on ScopeTypeNode)
                                exported_types.insert(
                                    exported,
                                    ScopeTypeNode {
                                        owner_scope: scope.id,
                                        value: TypeOrDecl::Type(Rc::from(TsType::TsTypeRef(
                                            TsTypeRef {
                                                span: DUMMY_SP,
                                                type_name: TsEntityName::Ident(Ident {
                                                    span: DUMMY_SP,
                                                    sym: local,
                                                    optional: false,
                                                }),
                                                type_params: None,
                                            },
                                        ))),
                                    },
                                );
                            } else if let Some(local_type) = types.get(&local) {
                                // exporting local defined type
                                exported_types.insert(exported, local_type.to_owned());
                            }
                        }
                    }

                    ModuleDecl::ExportAll(_) => {
                        // TODO This is not yet supported
                        // const sourceScope = importSourceToScope(
                        //   ctx,
                        //   stmt.source,
                        //   scope,
                        //   stmt.source.value,
                        // )
                        // Object.assign(scope.exportedTypes, sourceScope.exportedTypes)
                    }

                    ModuleDecl::ExportDefaultDecl(decl) => {
                        match decl.decl {
                            DefaultDecl::TsInterfaceDecl(ref interface_decl) => {
                                record_type_interface_decl(interface_decl, types, declares);
                                record_type_interface_decl(
                                    interface_decl,
                                    exported_types,
                                    exported_declares,
                                );
                            }

                            DefaultDecl::Class(ref class) => {
                                record_type_class(
                                    &class.class,
                                    class.ident.as_ref(),
                                    types,
                                    Some(fervid_atom!("default")),
                                );
                                record_type_class(
                                    &class.class,
                                    class.ident.as_ref(),
                                    exported_types,
                                    Some(fervid_atom!("default")),
                                );
                            }

                            DefaultDecl::Fn(ref fn_decl) => {
                                // todo
                            }
                        }
                    }

                    ModuleDecl::ExportDefaultExpr(expr) => {
                        // Only e.g. `export default foo` is processed
                        let Expr::Ident(ident) = expr.expr.as_ref() else {
                            continue;
                        };

                        if let Some(existing_type) = types.get(&ident.sym) {
                            exported_types
                                .insert(fervid_atom!("default"), existing_type.to_owned());
                        }
                    }

                    _ => {}
                },

                ModuleItem::Stmt(_) => {}
            }
        }
    }

    // TODO Support both `_ownerScope` and `_ns` (using IDs)
    // for (const key of Object.keys(types)) {
    //     const node = types[key]
    //     node._ownerScope = scope
    //     if (node._ns) node._ns._ownerScope = scope
    // }

    // TODO Support declares `_ownerScope`
    // for (const key of Object.keys(declares)) {
    //     declares[key]._ownerScope = scope
    // }
}

fn record_type_module_item(
    module_item: &ModuleItem,
    types: &mut HashMap<FervidAtom, ScopeTypeNode>,
    declares: &mut HashMap<FervidAtom, ScopeTypeNode>,
    overwrite_id: Option<FervidAtom>,
) {
    match module_item {
        ModuleItem::ModuleDecl(_) => {}
        ModuleItem::Stmt(stmt) => record_type_stmt(stmt, types, declares, overwrite_id),
    }
}

fn record_type_stmt(
    s: &Stmt,
    types: &mut HashMap<FervidAtom, ScopeTypeNode>,
    declares: &mut HashMap<FervidAtom, ScopeTypeNode>,
    overwrite_id: Option<FervidAtom>,
) {
    match s {
        Stmt::Decl(decl) => record_type_decl(decl, types, declares, overwrite_id),
        _ => {}
    }
}

fn record_type_decl(
    decl: &Decl,
    types: &mut HashMap<FervidAtom, ScopeTypeNode>,
    declares: &mut HashMap<FervidAtom, ScopeTypeNode>,
    overwrite_id: Option<FervidAtom>,
) {
    match decl {
        Decl::Class(class) => {
            record_type_class(&class.class, Some(&class.ident), types, overwrite_id)
        }

        Decl::TsInterface(ts_interface) => {
            let id = overwrite_id.unwrap_or_else(|| ts_interface.id.sym.to_owned());

            let Some(_existing) = types.get_mut(&id) else {
                types.insert(
                    id,
                    ScopeTypeNode {
                        value: TypeOrDecl::Decl(Rc::new(decl.clone())),
                        owner_scope: 0, // TODO
                    },
                );
                return;
            };

            // TODO Existing
        }

        Decl::TsEnum(ts_enum) => {
            let id = overwrite_id.unwrap_or_else(|| ts_enum.id.sym.to_owned());

            let Some(_existing) = types.get_mut(&id) else {
                types.insert(
                    id,
                    ScopeTypeNode {
                        value: TypeOrDecl::Decl(Rc::new(decl.clone())),
                        owner_scope: 0, // TODO
                    },
                );
                return;
            };

            // TODO Existing
        }

        Decl::TsModule(ts_module) => {
            let id = overwrite_id.unwrap_or_else(|| match &ts_module.id {
                TsModuleName::Ident(id) => id.sym.to_owned(),
                TsModuleName::Str(s) => s.value.to_owned(),
            });

            let Some(_existing) = types.get_mut(&id) else {
                types.insert(
                    id,
                    ScopeTypeNode {
                        value: TypeOrDecl::Decl(Rc::new(decl.clone())),
                        owner_scope: 0, // TODO
                    },
                );
                return;
            };

            // TODO Existing
        }

        Decl::TsTypeAlias(ts_type_alias) => {
            let to_insert = if ts_type_alias.type_params.is_some() {
                TypeOrDecl::Decl(Rc::new(decl.to_owned()))
            } else {
                TypeOrDecl::Type(Rc::from(ts_type_alias.type_ann.clone()))
            };

            types.insert(
                ts_type_alias.id.sym.to_owned(),
                ScopeTypeNode {
                    value: to_insert,
                    owner_scope: 0, // TODO
                },
            );
        }

        Decl::Fn(fn_decl) => {
            // Shallow clone (without body)
            declares.insert(
                fn_decl.ident.sym.to_owned(),
                ScopeTypeNode {
                    value: TypeOrDecl::Decl(Rc::new(Decl::Fn(FnDecl {
                        ident: fn_decl.ident.to_owned(),
                        declare: fn_decl.declare,
                        function: Box::new(Function {
                            params: fn_decl.function.params.clone(),
                            decorators: vec![],
                            span: fn_decl.function.span,
                            body: None,
                            is_generator: fn_decl.function.is_generator,
                            is_async: fn_decl.function.is_generator,
                            type_params: fn_decl.function.type_params.clone(),
                            return_type: fn_decl.function.return_type.clone(),
                        }),
                    }))),
                    owner_scope: 0, // TODO
                },
            );
        }

        Decl::Var(var_decl) => {
            if !var_decl.declare {
                return;
            }

            for decl in var_decl.decls.iter() {
                let Pat::Ident(ref ident) = decl.name else {
                    continue;
                };

                let Some(ref type_ann) = ident.type_ann else {
                    continue;
                };

                declares.insert(
                    ident.sym.to_owned(),
                    ScopeTypeNode {
                        value: TypeOrDecl::Type(Rc::from(type_ann.type_ann.clone())),
                        owner_scope: 0, // TODO
                    },
                );
            }
        }

        Decl::Using(_) => {}
    }
}

fn record_type_interface_decl(
    interface_decl: &TsInterfaceDecl,
    types: &mut HashMap<FervidAtom, ScopeTypeNode>,
    declares: &mut HashMap<FervidAtom, ScopeTypeNode>,
) {
    // todo
}

fn record_type_class(
    class: &Class,
    ident: Option<&Ident>,
    types: &mut HashMap<FervidAtom, ScopeTypeNode>,
    overwrite_id: Option<FervidAtom>,
) {
    // Overwrite or ident
    let id = overwrite_id.or_else(|| ident.map(|v| v.sym.clone()));
    if let Some(id) = id {
        // Shallow copy
        types.insert(
            id.clone(),
            ScopeTypeNode {
                owner_scope: 0, // TODO
                value: TypeOrDecl::Decl(Rc::from(Decl::Class(ClassDecl {
                    ident: Ident {
                        span: DUMMY_SP,
                        sym: id,
                        optional: false,
                    },
                    declare: false,
                    class: Box::new(Class {
                        span: class.span,
                        decorators: vec![],
                        body: vec![],
                        super_class: None,
                        is_abstract: class.is_abstract,
                        type_params: None,
                        super_type_params: None,
                        implements: vec![],
                    }),
                }))),
            },
        );
    }
}

#[inline]
fn is_declare(module_item: &ModuleItem) -> bool {
    match module_item {
        ModuleItem::ModuleDecl(_) => false,
        ModuleItem::Stmt(stmt) => match stmt {
            Stmt::Decl(d) => match d {
                Decl::Class(c) => c.declare,
                Decl::Fn(f) => f.declare,
                Decl::Var(v) => v.declare,
                Decl::Using(_) => false,
                Decl::TsInterface(i) => i.declare,
                Decl::TsTypeAlias(t) => t.declare,
                Decl::TsEnum(e) => e.declare,
                Decl::TsModule(m) => m.declare,
            },
            _ => false,
        },
    }
}

flagset::flags! {
    #[derive(AsRefStr, EnumString, IntoStaticStr)]
    pub enum Types: usize {
        String,
        Number,
        Boolean,
        Object,
        #[strum(serialize = "null")]
        Null,
        Unknown,
        Function,
        Array,
        Set,
        Map,
        WeakSet,
        WeakMap,
        Date,
        Promise,
        Error,
        Symbol,
    }
}

pub type TypesSet = FlagSet<Types>;

pub fn infer_runtime_type(
    ctx: &mut TypeResolveContext,
    node: &ScopeTypeNode,
    scope: &TypeScope,
    is_key_of: bool,
) -> TypesSet {
    match node.value {
        TypeOrDecl::Type(ref ts_type) => infer_runtime_type_type(ctx, ts_type, scope, is_key_of),
        TypeOrDecl::Decl(ref decl) => infer_runtime_type_declaration(ctx, decl, scope, is_key_of),
    }
}

pub fn infer_runtime_type_type(
    ctx: &mut TypeResolveContext,
    ts_type: &TsType,
    scope: &TypeScope,
    is_key_of: bool,
) -> TypesSet {
    macro_rules! return_value {
        ($v: expr) => {
            FlagSet::from($v)
        };
    }

    match ts_type {
        TsType::TsKeywordType(keyword) => match keyword.kind {
            TsKeywordTypeKind::TsStringKeyword => return return_value!(Types::String),
            TsKeywordTypeKind::TsNumberKeyword | TsKeywordTypeKind::TsBigIntKeyword => {
                return return_value!(Types::Number)
            }
            TsKeywordTypeKind::TsBooleanKeyword => return return_value!(Types::Boolean),
            TsKeywordTypeKind::TsObjectKeyword => return return_value!(Types::Object),
            TsKeywordTypeKind::TsNullKeyword => return return_value!(Types::Null),

            TsKeywordTypeKind::TsAnyKeyword => {
                if is_key_of {
                    return return_value!(Types::String | Types::Number | Types::Symbol);
                }
            }
            TsKeywordTypeKind::TsSymbolKeyword => return return_value!(Types::Symbol),

            TsKeywordTypeKind::TsUnknownKeyword
            | TsKeywordTypeKind::TsVoidKeyword
            | TsKeywordTypeKind::TsUndefinedKeyword
            | TsKeywordTypeKind::TsNeverKeyword
            | TsKeywordTypeKind::TsIntrinsicKeyword => return return_value!(Types::Unknown),
        },

        TsType::TsTypeLit(type_lit) => {
            return infer_runtime_type_type_elements(ctx, &type_lit.members, scope, is_key_of);
        }

        TsType::TsFnOrConstructorType(_) => return return_value!(Types::Function),
        TsType::TsArrayType(_) | TsType::TsTupleType(_) => return return_value!(Types::Array),

        TsType::TsLitType(literal_type) => match literal_type.lit {
            TsLit::Number(_) => return return_value!(Types::Number),
            TsLit::Str(_) => return return_value!(Types::String),
            TsLit::Bool(_) => return return_value!(Types::Boolean),
            TsLit::BigInt(_) => return return_value!(Types::Number),
            TsLit::Tpl(_) => return return_value!(Types::Unknown),
        },

        TsType::TsTypeRef(type_ref) => 't: {
            let resolved = resolve_type_reference(ctx, ReferenceTypes::TsType(ts_type), scope);
            if let Some(resolved) = resolved {
                // TODO Use `resolved._ownerScope`
                return infer_runtime_type(ctx, &resolved, scope, is_key_of);
            }

            let TsEntityName::Ident(ref ident) = type_ref.type_name else {
                break 't;
            };

            if is_key_of {
                match ident.sym.as_str() {
                    "String"
                    | "Array"
                    | "ArrayLike"
                    | "Parameters"
                    | "ConstructorParameters"
                    | "ReadonlyArray" => {
                        return return_value!(Types::String | Types::Number);
                    }

                    // TS built-in utility types
                    "Record" | "Partial" | "Required" | "Readonly" => {
                        if let Some(first_type_param) =
                            type_ref.type_params.as_ref().and_then(|v| v.params.first())
                        {
                            return infer_runtime_type_type(ctx, &first_type_param, scope, true);
                        };
                    }
                    "Pick" | "Extract" => {
                        if let Some(second_type_param) =
                            type_ref.type_params.as_ref().and_then(|v| v.params.get(1))
                        {
                            return infer_runtime_type_type(ctx, &second_type_param, scope, false);
                        };
                    }

                    "Function" | "Object" | "Set" | "Map" | "WeakSet" | "WeakMap" | "Date"
                    | "Promise" | "Error" | "Uppercase" | "Lowercase" | "Capitalize"
                    | "Uncapitalize" | "ReadonlyMap" | "ReadonlySet" => {
                        return return_value!(Types::String);
                    }

                    _ => {}
                }
            } else {
                match ident.sym.as_str() {
                    "Array" => return return_value!(Types::Array),
                    "Function" => return return_value!(Types::Function),
                    "Object" => return return_value!(Types::Object),
                    "Set" => return return_value!(Types::Set),
                    "Map" => return return_value!(Types::Map),
                    "WeakSet" => return return_value!(Types::WeakSet),
                    "WeakMap" => return return_value!(Types::WeakMap),
                    "Date" => return return_value!(Types::Date),
                    "Promise" => return return_value!(Types::Promise),
                    "Error" => return return_value!(Types::Error),

                    // TS built-in utility types
                    // https://www.typescriptlang.org/docs/handbook/utility-types.html
                    "Partial" | "Required" | "Readonly" | "Record" | "Pick" | "Omit"
                    | "InstanceType" => {
                        return return_value!(Types::Object);
                    }

                    "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize" => {
                        return return_value!(Types::String);
                    }

                    "Parameters" | "ConstructorParameters" | "ReadonlyArray" => {
                        return return_value!(Types::Array);
                    }

                    "ReadonlyMap" => return return_value!(Types::Map),
                    "ReadonlySet" => return return_value!(Types::Set),

                    "NonNullable" => {
                        if let Some(first_type_param) =
                            type_ref.type_params.as_ref().and_then(|v| v.params.first())
                        {
                            let mut inferred =
                                infer_runtime_type_type(ctx, &first_type_param, scope, false);
                            inferred -= Types::Null;
                            return inferred;
                        };
                    }

                    "Extract" => {
                        if let Some(second_type_param) =
                            type_ref.type_params.as_ref().and_then(|v| v.params.get(1))
                        {
                            return infer_runtime_type_type(ctx, &second_type_param, scope, false);
                        };
                    }

                    "Exclude" | "OmitThisParameter" => {
                        if let Some(first_type_param) =
                            type_ref.type_params.as_ref().and_then(|v| v.params.first())
                        {
                            return infer_runtime_type_type(ctx, &first_type_param, scope, false);
                        };
                    }

                    _ => {}
                }
            }
        }

        TsType::TsParenthesizedType(paren) => {
            return infer_runtime_type_type(ctx, &paren.type_ann, scope, false);
        }

        TsType::TsUnionOrIntersectionType(union_or_intersection) => {
            let (types, is_intersection) = match union_or_intersection {
                TsUnionOrIntersectionType::TsUnionType(union_type) => (&union_type.types, false),
                TsUnionOrIntersectionType::TsIntersectionType(intersection) => {
                    (&intersection.types, true)
                }
            };
            let mut flattened = flatten_types(ctx, &types, scope, is_key_of);
            if is_intersection {
                flattened -= Types::Unknown;
            }
            return flattened;
        }

        TsType::TsIndexedAccessType(index_type) => {
            let Ok(types) = resolve_index_type(ctx, index_type, scope) else {
                // Soft-fail
                return return_value!(Types::Unknown);
            };
            return flatten_types(ctx, &types, scope, false);
        }

        TsType::TsImportType(_) => {
            // TODO
        },

        TsType::TsTypeQuery(type_query) => 't: {
            let TsTypeQueryExpr::TsEntityName(TsEntityName::Ident(ref ident)) =
                type_query.expr_name
            else {
                break 't;
            };

            let matched = scope.declares.get(&ident.sym);
            if let Some(matched) = matched {
                // TODO Switch scope to the `matched._ownerScope`
                return infer_runtime_type(ctx, matched, scope, is_key_of);
            }
        }

        // `keyof`, `unique`, `readonly`
        TsType::TsTypeOperator(type_operator) => {
            let is_key_of = matches!(type_operator.op, TsTypeOperatorOp::KeyOf);
            return infer_runtime_type_type(ctx, &type_operator.type_ann, scope, is_key_of);
        }

        _ => {}
    }

    // No runtime check at this point
    FlagSet::from(Types::Unknown)
}

pub fn infer_runtime_type_declaration(
    ctx: &mut TypeResolveContext,
    decl: &Decl,
    scope: &TypeScope,
    is_key_of: bool,
) -> TypesSet {
    match decl {
        Decl::TsInterface(interface) => {
            infer_runtime_type_type_elements(ctx, &interface.body.body, scope, is_key_of)
        }
        Decl::TsEnum(ts_enum) => infer_enum_type(ts_enum),
        Decl::Class(_) => TypesSet::from(Types::Object),
        _ => TypesSet::from(Types::Unknown),
    }
}

fn infer_runtime_type_type_elements(
    ctx: &mut TypeResolveContext,
    elements: &[TsTypeElement],
    scope: &TypeScope,
    is_key_of: bool,
) -> TypesSet {
    let mut result: TypesSet = FlagSet::default();

    for member in elements.iter() {
        if !is_key_of {
            let call_or_construct = matches!(
                member,
                TsTypeElement::TsCallSignatureDecl(_) | TsTypeElement::TsConstructSignatureDecl(_)
            );

            result |= if call_or_construct {
                Types::Function
            } else {
                Types::Object
            };

            continue;
        }

        match member {
            TsTypeElement::TsPropertySignature(property_signature)
                if matches!(property_signature.key.as_ref(), Expr::Lit(Lit::Num(_))) =>
            {
                result |= Types::Number;
            }

            TsTypeElement::TsIndexSignature(index_signature) => {
                let Some(first_param) = index_signature.params.first() else {
                    continue;
                };

                let type_ann = match first_param {
                    TsFnParam::Ident(i) => &i.type_ann,
                    TsFnParam::Array(a) => &a.type_ann,
                    TsFnParam::Rest(r) => &r.type_ann,
                    TsFnParam::Object(o) => &o.type_ann,
                };

                let Some(annotation) = type_ann else {
                    continue;
                };

                // Here official compiler assumes only one element in the set
                let inferred = infer_runtime_type_type(ctx, &annotation.type_ann, scope, false);
                if inferred.contains(Types::Unknown) {
                    return TypesSet::from(Types::Unknown);
                }
                result |= inferred;
            }

            _ => {
                result |= Types::String;
            }
        }
    }

    if result.is_empty() {
        result |= if is_key_of {
            Types::Unknown
        } else {
            Types::Object
        };
    }

    return result;
}

pub fn infer_runtime_type_type_element(
    ctx: &mut TypeResolveContext,
    ts_type_element: &TsTypeElement,
    scope: &TypeScope,
) -> TypesSet {
    macro_rules! unknown {
        () => {
            FlagSet::from(Types::Unknown)
        };
    }

    let type_ann = match ts_type_element {
        TsTypeElement::TsCallSignatureDecl(d) => &d.type_ann,
        TsTypeElement::TsConstructSignatureDecl(d) => &d.type_ann,
        TsTypeElement::TsPropertySignature(s) => &s.type_ann,
        TsTypeElement::TsGetterSignature(s) => &s.type_ann,
        TsTypeElement::TsSetterSignature(s) => return unknown!(),
        TsTypeElement::TsMethodSignature(s) => &s.type_ann,
        TsTypeElement::TsIndexSignature(s) => &s.type_ann,
    };

    let Some(type_ann) = type_ann else {
        return unknown!();
    };

    infer_runtime_type_type(ctx, &type_ann.type_ann, scope, false)
}

fn flatten_types(
    ctx: &mut TypeResolveContext,
    types: &[Box<TsType>],
    scope: &TypeScope,
    is_key_of: bool,
) -> TypesSet {
    let mut result = FlagSet::<Types>::default();
    for ts_type in types {
        result |= infer_runtime_type_type(ctx, &ts_type, scope, is_key_of);
    }
    result
}

fn infer_enum_type(ts_enum: &TsEnumDecl) -> TypesSet {
    let mut result = TypesSet::default();

    for m in ts_enum.members.iter() {
        let Some(ref initializer) = m.init else {
            continue;
        };

        let Expr::Lit(ref lit) = initializer.as_ref() else {
            continue;
        };

        match lit {
            Lit::Str(_) => result |= Types::String,
            Lit::Num(_) => result |= Types::Number,
            _ => {}
        }
    }

    if result.is_empty() {
        result |= Types::Number;
    }

    result
}

/// Support for the `ExtractPropTypes` helper - it's non-exhaustive, mostly
/// tailored towards popular component libs like element-plus and antd-vue.
fn resolve_extract_prop_types(
    ctx: &TypeResolveContext,
    mut resolved_elements: ResolvedElements,
) -> ResolutionResult<ResolvedElements> {
    // Reuse the same object, so clear `calls` just to be compatible to the official compiler
    resolved_elements.calls.clear();

    for raw in resolved_elements.props.values_mut() {
        let (key, type_ann) = match raw {
            TsTypeElement::TsPropertySignature(ref s) => {
                (&s.key, s.type_ann.as_ref().map(|v| &v.type_ann))
            }
            TsTypeElement::TsMethodSignature(ref s) => {
                (&s.key, s.type_ann.as_ref().map(|v| &v.type_ann))
            }
            x => {
                return Err(error(ScriptErrorKind::ResolveTypeUnresolvable, x.span()));
            }
        };

        if let Some(type_ann) = type_ann {
            *raw = reverse_infer_type(ctx, &key, &type_ann);
        } else {
            return Err(error(ScriptErrorKind::ResolveTypeUnresolvable, raw.span()));
        }
    }

    Ok(resolved_elements)
}

#[inline]
fn reverse_infer_type(ctx: &TypeResolveContext, expr: &Expr, type_ann: &TsType) -> TsTypeElement {
    reverse_infer_type_impl(ctx, expr, type_ann, true, true)
}

fn reverse_infer_type_impl(
    ctx: &TypeResolveContext,
    key: &Expr,
    ts_type: &TsType,
    optional: bool,
    check_object_syntax: bool,
) -> TsTypeElement {
    if let (true, TsType::TsTypeLit(type_lit)) = (check_object_syntax, ts_type) {
        // check { type: xxx }
        let type_type = find_static_property_type(type_lit, "type");
        if let Some(type_type) = type_type {
            let required_type = find_static_property_type(type_lit, "required");

            let optional = match required_type {
                Some(TsType::TsLitType(lit_type)) => {
                    if let TsLit::Bool(b) = lit_type.lit {
                        !b.value
                    } else {
                        true
                    }
                }
                _ => false,
            };

            return reverse_infer_type_impl(ctx, key, type_type, optional, false);
        }
    }

    match ts_type {
        TsType::TsTypeRef(type_ref) => match type_ref.type_name {
            TsEntityName::Ident(ref type_ref_ident) => {
                let type_name = type_ref_ident.sym.as_str();

                if type_name.ends_with("Constructor") {
                    return TsTypeElement::TsGetterSignature(create_property(
                        Box::new(key.to_owned()),
                        ctor_to_type(type_name),
                        optional,
                    ));
                } else if let ("PropType", Some(type_params)) =
                    (type_name, type_ref.type_params.as_ref())
                {
                    if let Some(first_type_param) = type_params.params.first() {
                        // PropType<{}>
                        return TsTypeElement::TsGetterSignature(create_property(
                            Box::new(key.to_owned()),
                            first_type_param.to_owned(),
                            optional,
                        ));
                    }
                }
            }

            TsEntityName::TsQualifiedName(_) => {
                if let Some(first_type_param) =
                    type_ref.type_params.as_ref().and_then(|v| v.params.first())
                {
                    // NOTE: here the iteration over params was used in the original implementation,
                    // but in reality it will always return on the first param:
                    // https://github.com/vuejs/core/blob/422ef34e487f801e1162bed80c0e88e868576e1d/packages/compiler-sfc/src/script/resolveType.ts#L1857-L1860

                    return reverse_infer_type_impl(ctx, key, first_type_param, optional, true);
                }
            }
        },

        TsType::TsImportType(import_type) => {
            if let Some(first_type_param) = import_type
                .type_args
                .as_ref()
                .and_then(|v| v.params.first())
            {
                // try if we can catch Foo.Bar<XXXConstructor>

                // NOTE: here the iteration over params was used in the original implementation,
                // but in reality it will always return on the first param:
                // https://github.com/vuejs/core/blob/422ef34e487f801e1162bed80c0e88e868576e1d/packages/compiler-sfc/src/script/resolveType.ts#L1857-L1860

                return reverse_infer_type_impl(ctx, key, first_type_param, optional, true);
            }
        }

        _ => {}
    }

    // When couldn't infer, simply return `null`
    TsTypeElement::TsGetterSignature(create_property(
        Box::new(key.to_owned()),
        Box::new(TsType::TsKeywordType(TsKeywordType {
            span: DUMMY_SP,
            kind: TsKeywordTypeKind::TsNullKeyword,
        })),
        optional,
    ))
}

fn get_id(expr: &Expr) -> Option<FervidAtom> {
    match expr {
        Expr::Ident(ident) => Some(ident.sym.to_owned()),
        Expr::Lit(Lit::Str(s)) => Some(s.value.to_owned()),
        _ => None,
    }
}

fn create_property(
    key: Box<Expr>,
    type_annotation: Box<TsType>,
    optional: bool,
) -> TsGetterSignature {
    TsGetterSignature {
        span: DUMMY_SP,
        readonly: false,
        key,
        computed: false,
        optional,
        type_ann: Some(Box::new(TsTypeAnn {
            span: DUMMY_SP,
            type_ann: type_annotation,
        })),
    }
}

fn ctor_to_type(ctor_type: &str) -> Box<TsType> {
    // It is fine to omit UTF8 checks from here,
    // because this function is called when `ctor_type` ends with `Constructor` (11 chars long).
    let end_idx = ctor_type.len().saturating_sub(11);
    let ctor = &ctor_type[..end_idx];

    macro_rules! keyword {
        ($type_kind: ident) => {
            Box::new(TsType::TsKeywordType(TsKeywordType {
                span: DUMMY_SP,
                kind: TsKeywordTypeKind::$type_kind,
            }))
        };
    }

    match ctor {
        "String" => keyword!(TsStringKeyword),
        "Number" => keyword!(TsNumberKeyword),
        "Boolean" => keyword!(TsBooleanKeyword),

        "Array" | "Function" | "Object" | "Set" | "Map" | "WeakSet" | "WeakMap" | "Date"
        | "Promise" => Box::new(TsType::TsTypeRef(TsTypeRef {
            span: DUMMY_SP,
            type_name: TsEntityName::Ident(Ident {
                span: DUMMY_SP,
                sym: FervidAtom::from(ctor),
                optional: false,
            }),
            type_params: None,
        })),

        // Fallback to null
        _ => keyword!(TsNullKeyword),
    }
}

fn capitalize_or_uncapitalize_atoms(atoms: &mut Vec<FervidAtom>, capitalize: bool) {
    for atom in atoms.iter_mut() {
        let mut buf = String::with_capacity(atom.len());
        if let Some(c) = atom.chars().next() {
            let transformed = if capitalize {
                itertools::Either::Left(c.to_uppercase())
            } else {
                itertools::Either::Right(c.to_lowercase())
            };
            for c_transformed in transformed {
                buf.push(c_transformed);
            }
            buf.push_str(&atom[c.len_utf8()..]);
        }
        *atom = FervidAtom::from(buf);
    }
}

#[inline]
fn error(kind: ScriptErrorKind, span: Span) -> ScriptError {
    ScriptError { span, kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::parser::parse_typescript_expr;

    #[test]
    fn it_resolves_template_literal_keys() {
        let expr = parse_typescript_expr(
            "`${'foo' | 'bar' | 'baz'}2${'baz' | 'qux'}3${'2'}`",
            0,
            Default::default(),
        )
        .expect("Should parse")
        .0
        .expect_tpl();

        let result = resolve_template_keys(&expr).expect("Should not error");

        assert_eq!(
            result,
            vec![
                "baz2qux32",
                "baz2baz32",
                "bar2qux32",
                "bar2baz32",
                "foo2qux32",
                "foo2baz32"
            ]
        );
    }

    #[test]
    fn it_resolves_qualified_names() {
        // A.B.C
        let a_b_c = TsQualifiedName {
            left: TsEntityName::TsQualifiedName(Box::new(TsQualifiedName {
                left: TsEntityName::Ident(Ident::new("A".into(), DUMMY_SP)),
                right: Ident::new("B".into(), DUMMY_SP),
            })),
            right: Ident::new("C".into(), DUMMY_SP),
        };

        let result = qualified_name_to_path(&a_b_c);

        assert_eq!(result, vec!["A", "B", "C"]);
    }

    #[test]
    fn it_capitalizes() {
        let mut atoms = vec!["foo".into(), "bazBar".into(), "".into()];
        capitalize_or_uncapitalize_atoms(&mut atoms, true);
        assert_eq!(atoms, vec!["Foo", "BazBar", ""]);
    }

    #[test]
    fn it_uncapitalizes() {
        let mut atoms = vec!["Foo".into(), "BazBar".into(), "".into()];
        capitalize_or_uncapitalize_atoms(&mut atoms, false);
        assert_eq!(atoms, vec!["foo", "bazBar", ""]);
    }
}
