use crate::{AttributeOrBinding, VBindDirective, StrOrExpr};

/// Checks whether the attributes name is the same as `expected_name`
#[inline]
pub fn check_attribute_name(attr: &AttributeOrBinding, expected_name: &str) -> bool {
    matches!(attr,
        AttributeOrBinding::RegularAttribute { name, .. } |
        AttributeOrBinding::VBind(VBindDirective { argument: Some(StrOrExpr::Str(name)), .. })
        if name == expected_name
    )
}
