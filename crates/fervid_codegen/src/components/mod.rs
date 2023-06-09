use fervid_core::{ElementNode, VDirective};
use swc_core::{
    common::DUMMY_SP,
    ecma::{ast::ObjectLit, atoms::JsWord},
};

use crate::{attributes::DirectivesToProcess, context::CodegenContext};

impl CodegenContext {
    pub fn generate_component_vnode(component_node: &ElementNode, wrap_in_block: bool) {
        todo!()
    }

    fn generate_component_attributes<'e>(
        &mut self,
        component_node: &'e ElementNode,
    ) -> (ObjectLit, DirectivesToProcess<'e>) {
        let mut result_props = Vec::new();
        let mut remaining_directives = DirectivesToProcess::new();

        self.generate_attributes(
            &component_node.starting_tag.attributes,
            &mut result_props,
            &mut remaining_directives,
            component_node.template_scope,
        );

        // Process v-models
        remaining_directives.retain(|directive| match directive {
            VDirective::Model(v_model) => {
                self.generate_v_model_for_component(
                    v_model,
                    &mut result_props,
                    component_node.template_scope,
                );
                false
            }

            _ => true,
        });

        // TODO Take the remaining_directives and call a forwarding function
        // Process directives and hints wrt the createVNode

        let result = ObjectLit {
            span: DUMMY_SP, // todo from the component_node
            props: result_props,
        };

        (result, remaining_directives)
    }

    fn generate_component_children(component_node: &ElementNode) -> ObjectLit {
        let result = ObjectLit {
            span: DUMMY_SP, // TODO use span from the ElementNode
            props: vec![],
        };

        result
    }

    /// Creates the SWC identifier from a tag name. Will fetch from cache if present
    fn get_component_identifier(&mut self, tag_name: &str) -> JsWord {
        // Cached
        let existing_component_name = self.components.get(tag_name);
        if let Some(component_name) = existing_component_name {
            return component_name.to_owned();
        }

        // _component_ prefix plus tag name
        let mut component_name = tag_name.replace('-', "_");
        component_name.insert_str(0, "_component_");

        // To create an identifier, we need to convert it to an SWC JsWord
        let component_name = JsWord::from(component_name);

        self.components
            .insert(tag_name.to_owned(), component_name.to_owned());

        return component_name;
    }
}
