use swc_core::{
    atoms::Atom,
    common::DUMMY_SP,
    ecma::ast::{
        Expr, GetterProp, Ident, KeyValueProp, Lit, ObjectLit, Prop, PropName, PropOrSpread, Str,
    },
};

pub fn infer_name(exported_obj: &mut ObjectLit, filename: &str) {
    // Look for a user-defined `name`
    let is_defined = exported_obj.props.iter().any(|prop| {
        let PropOrSpread::Prop(prop) = prop else {
            return false;
        };

        match prop.as_ref() {
            Prop::Shorthand(s) if is_valid_name_sym(&s.sym) => true,
            Prop::KeyValue(KeyValueProp { key, .. }) | Prop::Getter(GetterProp { key, .. }) => {
                match key {
                    PropName::Ident(id) if is_valid_name_sym(&id.sym) => true,
                    PropName::Str(s) if is_valid_name_sym(&s.value) => true,
                    _ => false,
                }
            }
            _ => false,
        }
    });

    if is_defined {
        return;
    }

    // Remove `.vue` from the end of the filename
    let name_without_ext = filename.strip_suffix(".vue").unwrap_or(filename);

    // Add `__name` to the exported object
    exported_obj
        .props
        .push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(Ident {
                span: DUMMY_SP,
                sym: "__name".into(),
                optional: false,
            }),
            value: Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: name_without_ext.into(),
                raw: None,
            }))),
        }))))
}

#[inline]
fn is_valid_name_sym(sym: &Atom) -> bool {
    sym == "name" || sym == "__name"
}
