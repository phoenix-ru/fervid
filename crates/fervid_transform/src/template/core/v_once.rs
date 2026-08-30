use fervid_core::{CacheMarker, Node};

use crate::TransformSfcContext;

pub fn pre_transform_once(ctx: &mut TransformSfcContext, node: &mut Node) {
    let Node::Element(element_node) = node else {
        return;
    };

    if element_node
        .starting_tag
        .directives
        .as_ref()
        .is_some_and(|d| d.v_once.is_some())
    {
        ctx.directive_scopes.v_once += 1;
    }
}

pub fn post_transform_once(ctx: &mut TransformSfcContext, node: &mut Node) {
    let has_v_once = match node {
        Node::Element(element_node) => element_node
            .starting_tag
            .directives
            .as_ref()
            .is_some_and(|d| d.v_once.is_some()),
        Node::For(for_node) => for_node
            .codegen_node
            .as_ref()
            .is_some_and(|codegen| codegen.cache.contains(CacheMarker::InVOnce)),
        _ => false,
    };
    if !has_v_once {
        return;
    }

    ctx.directive_scopes.v_once -= 1;

    match node {
        Node::Element(element_node) => {
            if let Some(codegen_node) = element_node.codegen_node.as_mut() {
                codegen_node.cache |= CacheMarker::NeedPauseTracking | CacheMarker::InVOnce;
            }
        }
        Node::For(for_node) => {
            if let Some(codegen_node) = for_node.codegen_node.as_mut() {
                codegen_node.cache |= CacheMarker::NeedPauseTracking;
            }
        }
        Node::Text(_, _)
        | Node::Interpolation(_)
        | Node::Comment(_, _)
        | Node::ConditionalSeq(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{
        CacheMarker, ForParseResult, Node, PatchFlags, VForDirective, VueDirectives,
    };
    use swc_core::common::DUMMY_SP;

    use crate::{
        TransformSfcContext,
        template::{core::v_for::pre_transform_for, node_transforms::TransformNodeState},
        test_utils::{element_from_tag, js},
    };

    use super::{post_transform_once, pre_transform_once};

    #[test]
    fn caches_for_codegen_after_structural_replacement() {
        let mut element = element_from_tag("div");
        element.starting_tag.directives = Some(Box::new(VueDirectives {
            v_once: Some(()),
            v_for: Some(VForDirective {
                parse_result: Box::new(ForParseResult {
                    source: js("items"),
                    value: js("item"),
                    key: None,
                    index: None,
                    finalized: false,
                    finalized_is_dynamic: false,
                }),
                patch_flags: PatchFlags::UnkeyedFragment.into(),
                span: DUMMY_SP,
            }),
            ..Default::default()
        }));
        let mut node = Node::Element(element);
        let mut ctx = TransformSfcContext::anonymous();
        let mut state = TransformNodeState::default();

        pre_transform_once(&mut ctx, &mut node);
        pre_transform_for(&mut ctx, &mut state, &mut node);
        post_transform_once(&mut ctx, &mut node);

        assert_eq!(ctx.directive_scopes.v_once, 0);
        let Node::For(for_node) = node else {
            panic!("v-for should replace the original element")
        };
        let codegen = for_node
            .codegen_node
            .expect("v-for should initialize codegen metadata");
        assert!(codegen.cache.contains(CacheMarker::NeedPauseTracking));
        assert!(codegen.cache.contains(CacheMarker::InVOnce));
        let [Node::Element(child)] = for_node.children.as_slice() else {
            panic!("non-template v-for should retain its element child")
        };
        assert!(
            child
                .starting_tag
                .directives
                .as_ref()
                .is_some_and(|directives| directives.v_once.is_none())
        );
    }
}
