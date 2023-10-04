use fervid_core::{AttributeOrBinding, FervidAtom, StartingTag, VueDirectives};
use swc_html_ast::Attribute;

use crate::{attributes::process_element_attributes, error::ParseError};

pub fn process_element_starting_tag(
    tag_name: FervidAtom,
    raw_attributes: Vec<Attribute>,
    errors: &mut Vec<ParseError>,
) -> StartingTag {
    // Pre-allocate with excess, assuming all the attributes are not directives
    let mut attributes = Vec::<AttributeOrBinding>::with_capacity(raw_attributes.len());
    let mut directives = Option::<Box<VueDirectives>>::None;

    process_element_attributes(raw_attributes, &mut attributes, &mut directives, errors);

    StartingTag {
        tag_name,
        attributes,
        directives,
    }
}
