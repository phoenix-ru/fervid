use crate::{
    compiler::{codegen::CodegenContext, helper::CodeHelper, imports::VueImports},
    parser::attributes::HtmlAttribute,
    ElementNode,
};

impl CodegenContext<'_> {
    pub fn compile_slot(&mut self, buf: &mut String, element_node: &ElementNode) {
        buf.push_str(self.get_and_add_import_str(VueImports::RenderSlot));
        CodeHelper::open_paren(buf);

        // TODO is $slots always coming from _ctx?
        buf.push_str("_ctx.$slots, ");

        // Name of the slot (option because we'll need it one more time)
        let slot_name_option = element_node
            .starting_tag
            .attributes
            .iter()
            .find_map(|attr| match attr {
                HtmlAttribute::Regular {
                    name: "name",
                    value,
                } => Some(*value),
                _ => None,
            });
        let slot_name = slot_name_option.unwrap_or("default");
        CodeHelper::quoted(buf, slot_name);

        // Has attributes: attributes length is > 1 if `name` is present, > 0 otherwise
        let has_attributes =
            element_node.starting_tag.attributes.len() > slot_name_option.map_or(0, |_| 1);

        // Has children. Check to avoid generating extra
        let has_children = element_node.children.len() > 0;

        macro_rules! cleanup_return {
            () => {
                CodeHelper::close_paren(buf);
                return;
            };
        }

        // Early exit: no attributes and no children
        if !has_attributes || !has_children {
            cleanup_return!();
        }

        // When there are attributes other than name, generate them
        CodeHelper::comma(buf);
        if has_attributes {
            let filtered_attributes = element_node
                .starting_tag
                .attributes
                .iter()
                .filter(|attr| !matches!(attr, HtmlAttribute::Regular { name: "name", .. }));

            self.generate_attributes(buf, filtered_attributes, true, element_node.template_scope);
        } else if has_children {
            // When there are no attributes, but children present, push empty Js array
            buf.push_str("{}");
        }

        // No children work needed
        if !has_children {
            cleanup_return!();
        }

        // Generate children
        CodeHelper::comma(buf);
        buf.push_str("() => ");
        self.generate_element_children(
            buf,
            &element_node.children,
            false,
            element_node.template_scope,
        );

        cleanup_return!();
    }
}
