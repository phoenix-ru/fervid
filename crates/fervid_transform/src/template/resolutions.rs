use fervid_core::{BindingTypes, ComponentBinding, CustomDirectiveBinding, FervidAtom, IntoIdent};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, IdentName, MemberExpr, MemberProp},
};

use crate::{BindingsHelper, SetupBinding};

use super::{
    ast_transform::TemplateVisitor,
    expr_transform::BindingsHelperTransform,
    utils::{to_camel_case, to_pascal_case},
};

impl TemplateVisitor<'_> {
    /// Fuzzy-matches the component name to a binding name
    pub fn maybe_resolve_component(&mut self, tag_name: &FervidAtom) {
        // Check the existing resolutions.
        // Do nothing if found, regardless if it was previously resolved or not,
        // because codegen will handle the runtime resolution.
        if self.ctx.bindings_helper.components.contains_key(tag_name) {
            return;
        }

        // If the tag name contains a dot, it won't be found in the bindings - look directly for a namespaced component
        // Example: `<Foo.Bar>`
        let namespace_dot_idx = tag_name.find('.');
        let found = match namespace_dot_idx {
            Some(dot_idx) => find_binding(&mut self.ctx.bindings_helper, &tag_name[..dot_idx]),
            None => find_binding(&mut self.ctx.bindings_helper, tag_name),
        };

        if let Some(found) = found {
            let mut resolved_to = Expr::Ident(found.sym.to_owned().into_ident());

            // For `Component` binding types, do not transform.
            // TODO I am not sure about `Imported` though,
            // the official compiler sees them as if `SetupMaybeRef` and transforms.
            if !matches!(found.binding_type, BindingTypes::Component) {
                self.ctx
                    .bindings_helper
                    .transform_expr(&mut resolved_to, self.current_scope);
            }

            // For namespaced components, add the second part (`Bar` in `<Foo.Bar>`)
            if let Some(dot_idx) = namespace_dot_idx {
                resolved_to = Expr::Member(MemberExpr {
                    span: DUMMY_SP,
                    obj: Box::new(resolved_to),
                    prop: MemberProp::Ident(IdentName {
                        span: DUMMY_SP,
                        sym: FervidAtom::from(&tag_name[(dot_idx + 1)..]),
                    }),
                })
            }

            // Was resolved
            self.ctx.bindings_helper.components.insert(
                tag_name.to_owned(),
                ComponentBinding::Resolved(Box::new(resolved_to)),
            );
        } else {
            // Was not resolved
            self.ctx
                .bindings_helper
                .components
                .insert(tag_name.to_owned(), ComponentBinding::Unresolved);
        }
    }

    /// Fuzzy-matches the directive name to a binding name
    pub fn maybe_resolve_directive(&mut self, directive_name: &FervidAtom) {
        // Check the existing resolutions.
        // Do nothing if found, regardless if it was previously resolved or not,
        // because codegen will handle the runtime resolution.
        if self
            .ctx
            .bindings_helper
            .custom_directives
            .contains_key(directive_name)
        {
            return;
        }

        // Some special symbols in the directive name just make it impossible to create a js variable
        if directive_name.chars().any(|c| c == '[' || c == ']') {
            return;
        }

        // Directive bindings should always have a name in format `vCustomDirective` or `VCustomDirective`
        let mut normalized = String::with_capacity(directive_name.len());
        to_pascal_case(directive_name, &mut normalized);

        let found = self.ctx.bindings_helper.setup_bindings.iter().find(
            |SetupBinding {
                 sym: name,
                 binding_type: _,
                 span: _,
             }| {
                (name.starts_with('v') || name.starts_with('V')) && name[1..] == normalized
            },
        );

        // TODO Auto-importing the directives can happen here

        if let Some(found) = found {
            let mut resolved_to = Expr::Ident(found.sym.to_owned().into_ident());

            // Transform the identifier
            self.ctx
                .bindings_helper
                .transform_expr(&mut resolved_to, self.current_scope);

            // Was resolved
            self.ctx.bindings_helper.custom_directives.insert(
                directive_name.to_owned(),
                CustomDirectiveBinding::Resolved(Box::new(resolved_to)),
            );
        } else {
            // Was not resolved
            self.ctx.bindings_helper.custom_directives.insert(
                directive_name.to_owned(),
                CustomDirectiveBinding::Unresolved,
            );
        }
    }
}

fn find_binding<'a, 'b>(
    bindings_helper: &'a mut BindingsHelper,
    tag_name: &'b str,
) -> Option<&'a SetupBinding> {
    // `component-name`s like that should be transformed to `ComponentName`s
    let mut searched_pascal = String::with_capacity(tag_name.len());
    to_pascal_case(tag_name, &mut searched_pascal);

    // and to `componentName`
    let mut searched_camel = String::with_capacity(tag_name.len());
    to_camel_case(tag_name, &mut searched_camel);

    bindings_helper
        .setup_bindings
        .iter()
        .find(|binding| binding.sym == searched_pascal || binding.sym == searched_camel)

    // TODO Auto-importing the components can happen here
}

#[cfg(test)]
mod tests {
    use fervid_core::fervid_atom;

    use crate::TransformSfcContext;

    use super::*;

    #[test]
    fn it_resolves_components_pascal_case() {
        // `TestComponent` binding
        let mut ctx = with_bindings(vec![SetupBinding::new(
            fervid_atom!("TestComponent"),
            BindingTypes::Component,
        )]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        // `<test-component>`
        let kebab_case = fervid_atom!("test-component");
        template_visitor.maybe_resolve_component(&kebab_case);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&kebab_case),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<TestComponent>`
        let pascal_case = fervid_atom!("TestComponent");
        template_visitor.maybe_resolve_component(&pascal_case);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&pascal_case),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<UnresolvedComponent>`
        let unresolved = fervid_atom!("UnresolvedComponent");
        template_visitor.maybe_resolve_component(&unresolved);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&unresolved),
            Some(ComponentBinding::Unresolved)
        ));
    }

    #[test]
    fn it_resolves_components_camel_case() {
        // `testComponent` binding
        let mut ctx = with_bindings(vec![SetupBinding::new(
            fervid_atom!("testComponent"),
            BindingTypes::Component,
        )]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        // `<test-component>`
        let kebab_case = fervid_atom!("test-component");
        template_visitor.maybe_resolve_component(&kebab_case);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&kebab_case),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<TestComponent>` is not recognized (same as in official compiler)
        let pascal_case = fervid_atom!("TestComponent");
        template_visitor.maybe_resolve_component(&pascal_case);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&pascal_case),
            Some(ComponentBinding::Unresolved)
        ));

        // `<UnresolvedComponent>`
        let unresolved = fervid_atom!("UnresolvedComponent");
        template_visitor.maybe_resolve_component(&unresolved);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&unresolved),
            Some(ComponentBinding::Unresolved)
        ));
    }

    #[test]
    fn it_resolves_components_one_word() {
        // `Foo` and `bar` bindings
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("Foo"), BindingTypes::Component),
            SetupBinding::new(fervid_atom!("bar"), BindingTypes::SetupMaybeRef),
        ]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        // `<Foo>`
        let foo_capital = fervid_atom!("Foo");
        template_visitor.maybe_resolve_component(&foo_capital);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&foo_capital),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<foo>`
        let foo_lower = fervid_atom!("foo");
        template_visitor.maybe_resolve_component(&foo_lower);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&foo_lower),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<Bar>` is not recognized (same as in official compiler)
        let bar_capital = fervid_atom!("Bar");
        template_visitor.maybe_resolve_component(&bar_capital);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&bar_capital),
            Some(ComponentBinding::Unresolved)
        ));

        // `<bar>`
        let bar_lower = fervid_atom!("bar");
        template_visitor.maybe_resolve_component(&bar_lower);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&bar_lower),
            Some(ComponentBinding::Resolved(_))
        ));
    }

    #[test]
    fn it_resolves_components_namespaced() {
        // `Foo` binding
        let mut ctx = with_bindings(vec![SetupBinding::new(
            fervid_atom!("Foo"),
            BindingTypes::Imported,
        )]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        // `<Foo.Bar>`
        let namespaced = fervid_atom!("Foo.Bar");
        template_visitor.maybe_resolve_component(&namespaced);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&namespaced),
            Some(ComponentBinding::Resolved(e)) if e.is_member()
        ));

        // `<foo.bar>`
        let namespaced_lower = fervid_atom!("foo.bar");
        template_visitor.maybe_resolve_component(&namespaced_lower);
        assert!(matches!(
            template_visitor.ctx.bindings_helper.components.get(&namespaced_lower),
            Some(ComponentBinding::Resolved(e)) if e.is_member()
        ));
    }

    #[test]
    fn it_resolves_directive_one_word() {
        // `vFoo` and `VBar` bindings
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("vFoo"), BindingTypes::SetupLet),
            SetupBinding::new(fervid_atom!("VBar"), BindingTypes::SetupConst),
        ]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! assert_resolved {
            ($atom: literal) => {{
                let v = fervid_atom!($atom);
                template_visitor.maybe_resolve_directive(&v);
                assert!(matches!(
                    template_visitor
                        .ctx
                        .bindings_helper
                        .custom_directives
                        .get(&v),
                    Some(CustomDirectiveBinding::Resolved(_))
                ));
            }};
        }

        assert_resolved!("foo"); // `v-foo`
        assert_resolved!("Foo"); // `v-Foo`
        assert_resolved!("bar"); // `v-bar`
        assert_resolved!("Bar"); // `v-Bar`
    }

    #[test]
    fn it_resolves_directive_multi_word() {
        // `VFooBar` and `vBazQux` bindings
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("VFooBar"), BindingTypes::Imported),
            SetupBinding::new(fervid_atom!("vBazQux"), BindingTypes::SetupMaybeRef),
        ]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! assert_resolved {
            ($atom: literal) => {{
                let v = fervid_atom!($atom);
                template_visitor.maybe_resolve_directive(&v);
                assert!(matches!(
                    template_visitor
                        .ctx
                        .bindings_helper
                        .custom_directives
                        .get(&v),
                    Some(CustomDirectiveBinding::Resolved(_))
                ));
            }};
        }

        assert_resolved!("foo-bar"); // `v-foo-bar`
        assert_resolved!("FooBar"); // `v-FooBar`
        assert_resolved!("baz-qux"); // `v-baz-qux`
        assert_resolved!("BazQux"); // `v-BazQux`
    }

    #[test]
    fn it_does_not_resolve_directive_without_prefix() {
        // `Foo`, `bar`, `bazQux` and `TestNotDirective` bindings
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("Foo"), BindingTypes::Imported),
            SetupBinding::new(fervid_atom!("bar"), BindingTypes::SetupLet),
            SetupBinding::new(fervid_atom!("bazQux"), BindingTypes::SetupMaybeRef),
            SetupBinding::new(fervid_atom!("TestNotDirective"), BindingTypes::SetupConst),
        ]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        macro_rules! assert_unresolved {
            ($atom: literal) => {{
                let v = fervid_atom!($atom);
                template_visitor.maybe_resolve_directive(&v);
                assert!(matches!(
                    template_visitor
                        .ctx
                        .bindings_helper
                        .custom_directives
                        .get(&v),
                    Some(CustomDirectiveBinding::Unresolved)
                ));
            }};
        }

        assert_unresolved!("foo"); // `v-foo`
        assert_unresolved!("Foo");
        assert_unresolved!("bar");
        assert_unresolved!("Bar");
        assert_unresolved!("baz-qux");
        assert_unresolved!("bazQux");
        assert_unresolved!("BazQux");
        assert_unresolved!("test-not-directive");
        assert_unresolved!("TestNotDirective");
    }

    /// https://github.com/vuejs/core/blob/272ab9fbdcb1af0535108b9f888e80d612f9171d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L380-L401
    #[test]
    fn referencing_scope_components_and_directives() {
        // import ChildComp from './Child.vue'
        // import SomeOtherComp from './Other.vue'
        // import vMyDir from './my-dir'
        let mut ctx = with_bindings(vec![
            SetupBinding::new(fervid_atom!("ChildComp"), BindingTypes::Component),
            SetupBinding::new(fervid_atom!("SomeOtherComp"), BindingTypes::Component),
            SetupBinding::new(fervid_atom!("vMyDir"), BindingTypes::Imported),
        ]);
        let mut template_visitor = TemplateVisitor::new(&mut ctx);

        // <div v-my-dir></div>
        let v_my_dir = fervid_atom!("my-dir");
        template_visitor.maybe_resolve_directive(&v_my_dir);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .custom_directives
                .get(&v_my_dir),
            Some(CustomDirectiveBinding::Resolved(_))
        ));

        // <ChildComp/>
        let child_comp = fervid_atom!("ChildComp");
        template_visitor.maybe_resolve_component(&child_comp);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&child_comp),
            Some(ComponentBinding::Resolved(_))
        ));

        // <some-other-comp/>
        let some_other_comp = fervid_atom!("some-other-comp");
        template_visitor.maybe_resolve_component(&some_other_comp);
        assert!(matches!(
            template_visitor
                .ctx
                .bindings_helper
                .components
                .get(&some_other_comp),
            Some(ComponentBinding::Resolved(_))
        ));
    }

    fn with_bindings(mut bindings: Vec<SetupBinding>) -> TransformSfcContext {
        let mut ctx = TransformSfcContext::anonymous();
        ctx.bindings_helper.setup_bindings.append(&mut bindings);
        ctx
    }
}
