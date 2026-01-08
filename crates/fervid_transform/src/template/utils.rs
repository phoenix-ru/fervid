use fervid_core::{
    AttributeOrBinding, BuiltinType, ElementNode, FervidAtom, StrOrExpr, VUE_BUILTINS,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{ArrowExpr, BindingIdent, BlockStmtOrExpr, Expr, Ident, Lit, Pat},
};

/// Note: the original implementation only handles Teleport, Suspense, KeepAlive and BaseTransition
pub fn is_core_component(tag: &str) -> Option<BuiltinType> {
    let built_in = VUE_BUILTINS.get(tag);
    match built_in {
        Some(
            BuiltinType::Teleport
            | BuiltinType::Suspense
            | BuiltinType::KeepAlive
            | BuiltinType::BaseTransition,
        ) => built_in.copied(),
        _ => None,
    }
}

pub fn find_prop<'a>(
    node: &'a ElementNode,
    name: &str,
    dynamic_only: bool,
    allow_empty: bool,
) -> Option<&'a AttributeOrBinding> {
    for attr_or_binding in node.starting_tag.attributes.iter() {
        match attr_or_binding {
            AttributeOrBinding::RegularAttribute {
                name: attr_name,
                value,
                ..
            } => {
                if dynamic_only {
                    continue;
                }
                if attr_name == name && (allow_empty || !value.is_empty()) {
                    return Some(attr_or_binding);
                }
            }
            AttributeOrBinding::VBind(v_bind_directive) => {
                if is_static_arg_of(v_bind_directive.argument.as_ref(), name) {
                    return Some(attr_or_binding);
                }
            }
            AttributeOrBinding::VOn(_) => {}
        }
    }

    None
}

pub fn is_static_arg_of(arg: Option<&StrOrExpr>, name: &str) -> bool {
    match arg {
        Some(StrOrExpr::Str(s)) if s == name => true,
        Some(StrOrExpr::Expr(expr)) => match expr.as_ref() {
            Expr::Lit(Lit::Str(s)) if s.value == name => true,
            _ => false,
        },
        _ => false,
    }
}

/// `foo-bar-baz` -> `FooBarBaz`
#[inline]
pub(crate) fn to_pascal_case(raw: &str, out: &mut String) {
    for word in raw.split('-') {
        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                out.push(ch_component);
            }

            // Push the rest of the word
            out.push_str(&word[ch.len_utf8()..]);
        }
    }
}

/// `foo-bar-baz` -> `fooBarBaz`
#[inline]
pub(crate) fn to_camel_case(raw: &str, out: &mut String) {
    for (idx, word) in raw.split('-').enumerate() {
        if idx == 0 {
            out.push_str(word);
            continue;
        }

        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                out.push(ch_component);
            }

            // Push the rest of the word
            out.push_str(&word[ch.len_utf8()..]);
        }
    }
}

/// Converts `-` and special characters in `name` to `_` and character codes.
/// `asset_type` is one of: 'component', 'directive' or 'filter'.
/// Note that this mismatches with JS implementation as it operates on UTF-8 characters and not UTF-16 code units.
/// https://github.com/vuejs/core/blob/aac7e1898907445c8f89b22047a9bfcf0a6e91b8/packages/compiler-core/src/utils.ts#L490-L498
pub fn to_valid_asset_id(name: &str, asset_type: &str) -> String {
    let mut out = String::with_capacity(name.len() + 16);
    out.push('_');
    out.push_str(asset_type);
    out.push('_');

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else if ch == '-' {
            out.push('_');
        } else {
            out.push_str(&(ch as u32).to_string());
        }
    }

    out
}

/// Wraps `expr` to `$event => (expr)`
#[inline]
pub fn wrap_in_event_arrow(expr: Box<Expr>) -> Box<Expr> {
    let evt_param = Pat::Ident(BindingIdent {
        id: Ident {
            span: DUMMY_SP,
            ctxt: Default::default(),
            sym: FervidAtom::from("$event"),
            optional: false,
        },
        type_ann: None,
    });

    Box::new(Expr::Arrow(ArrowExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        params: vec![evt_param],
        body: Box::new(BlockStmtOrExpr::Expr(expr)),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    }))
}
