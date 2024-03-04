use fervid_core::{
    BindingTypes, ComponentBinding, CustomDirectiveBinding, FervidAtom, SetupBinding,
};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, Ident},
};

use super::{ast_transform::TemplateVisitor, expr_transform::BindingsHelperTransform, utils::{to_camel_case, to_pascal_case}};

impl TemplateVisitor<'_> {
    /// Fuzzy-matches the component name to a binding name
    pub fn maybe_resolve_component(&mut self, tag_name: &FervidAtom) {
        // Check the existing resolutions.
        // Do nothing if found, regardless if it was previously resolved or not,
        // because codegen will handle the runtime resolution.
        if self.bindings_helper.components.contains_key(tag_name) {
            return;
        }

        // `component-name`s like that should be transformed to `ComponentName`s
        let mut searched_pascal = String::with_capacity(tag_name.len());
        to_pascal_case(tag_name, &mut searched_pascal);

        // and to `componentName`
        let mut searched_camel = String::with_capacity(tag_name.len());
        to_camel_case(tag_name, &mut searched_camel);

        let found = self
            .bindings_helper
            .setup_bindings
            .iter()
            .find(|binding| binding.0 == searched_pascal || binding.0 == searched_camel);

        // TODO Auto-importing the components can happen here

        if let Some(found) = found {
            let mut resolved_to = Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: found.0.to_owned(),
                optional: false,
            });

            // For `Component` binding types, do not transform.
            // TODO I am not sure about `Imported` though,
            // the official compiler sees them as if `SetupMaybeRef` and transforms.
            if !matches!(found.1, BindingTypes::Component) {
                self.bindings_helper
                    .transform_expr(&mut resolved_to, self.current_scope);
            }

            // Was resolved
            self.bindings_helper.components.insert(
                tag_name.to_owned(),
                ComponentBinding::Resolved(Box::new(resolved_to)),
            );
        } else {
            // Was not resolved
            self.bindings_helper
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

        let found = self
            .bindings_helper
            .setup_bindings
            .iter()
            .find(|SetupBinding(name, _)| {
                (name.starts_with('v') || name.starts_with('V')) && name[1..] == normalized
            });

        // TODO Auto-importing the directives can happen here

        if let Some(found) = found {
            let mut resolved_to = Expr::Ident(Ident {
                span: DUMMY_SP,
                sym: found.0.to_owned(),
                optional: false,
            });

            // Transform the identifier
            self.bindings_helper
                .transform_expr(&mut resolved_to, self.current_scope);

            // Was resolved
            self.bindings_helper.custom_directives.insert(
                directive_name.to_owned(),
                CustomDirectiveBinding::Resolved(Box::new(resolved_to)),
            );
        } else {
            // Was not resolved
            self.bindings_helper.custom_directives.insert(
                directive_name.to_owned(),
                CustomDirectiveBinding::Unresolved,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{fervid_atom, BindingsHelper, SetupBinding};

    use super::*;

    #[test]
    fn it_resolves_components_pascal_case() {
        // `TestComponent` binding
        let mut bindings_helper = with_bindings(vec![SetupBinding(
            fervid_atom!("TestComponent"),
            BindingTypes::Component,
        )]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        // `<test-component>`
        let kebab_case = fervid_atom!("test-component");
        template_visitor.maybe_resolve_component(&kebab_case);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&kebab_case),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<TestComponent>`
        let pascal_case = fervid_atom!("TestComponent");
        template_visitor.maybe_resolve_component(&pascal_case);
        assert!(matches!(
            template_visitor
                .bindings_helper
                .components
                .get(&pascal_case),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<UnresolvedComponent>`
        let unresolved = fervid_atom!("UnresolvedComponent");
        template_visitor.maybe_resolve_component(&unresolved);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&unresolved),
            Some(ComponentBinding::Unresolved)
        ));
    }

    #[test]
    fn it_resolves_components_camel_case() {
        // `testComponent` binding
        let mut bindings_helper = with_bindings(vec![SetupBinding(
            fervid_atom!("testComponent"),
            BindingTypes::Component,
        )]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        // `<test-component>`
        let kebab_case = fervid_atom!("test-component");
        template_visitor.maybe_resolve_component(&kebab_case);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&kebab_case),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<TestComponent>` is not recognized (same as in official compiler)
        let pascal_case = fervid_atom!("TestComponent");
        template_visitor.maybe_resolve_component(&pascal_case);
        assert!(matches!(
            template_visitor
                .bindings_helper
                .components
                .get(&pascal_case),
            Some(ComponentBinding::Unresolved)
        ));

        // `<UnresolvedComponent>`
        let unresolved = fervid_atom!("UnresolvedComponent");
        template_visitor.maybe_resolve_component(&unresolved);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&unresolved),
            Some(ComponentBinding::Unresolved)
        ));
    }

    #[test]
    fn it_resolves_components_one_word() {
        // `Foo` and `bar` bindings
        let mut bindings_helper = with_bindings(vec![
            SetupBinding(fervid_atom!("Foo"), BindingTypes::Component),
            SetupBinding(fervid_atom!("bar"), BindingTypes::SetupMaybeRef),
        ]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        // `<Foo>`
        let foo_capital = fervid_atom!("Foo");
        template_visitor.maybe_resolve_component(&foo_capital);
        assert!(matches!(
            template_visitor
                .bindings_helper
                .components
                .get(&foo_capital),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<foo>`
        let foo_lower = fervid_atom!("foo");
        template_visitor.maybe_resolve_component(&foo_lower);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&foo_lower),
            Some(ComponentBinding::Resolved(_))
        ));

        // `<Bar>` is not recognized (same as in official compiler)
        let bar_capital = fervid_atom!("Bar");
        template_visitor.maybe_resolve_component(&bar_capital);
        assert!(matches!(
            template_visitor
                .bindings_helper
                .components
                .get(&bar_capital),
            Some(ComponentBinding::Unresolved)
        ));

        // `<bar>`
        let bar_lower = fervid_atom!("bar");
        template_visitor.maybe_resolve_component(&bar_lower);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&bar_lower),
            Some(ComponentBinding::Resolved(_))
        ));
    }

    #[test]
    fn it_resolves_directive_one_word() {
        // `vFoo` and `VBar` bindings
        let mut bindings_helper = with_bindings(vec![
            SetupBinding(fervid_atom!("vFoo"), BindingTypes::SetupLet),
            SetupBinding(fervid_atom!("VBar"), BindingTypes::SetupConst),
        ]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        macro_rules! assert_resolved {
            ($atom: literal) => {{
                let v = fervid_atom!($atom);
                template_visitor.maybe_resolve_directive(&v);
                assert!(matches!(
                    template_visitor.bindings_helper.custom_directives.get(&v),
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
        let mut bindings_helper = with_bindings(vec![
            SetupBinding(fervid_atom!("VFooBar"), BindingTypes::Imported),
            SetupBinding(fervid_atom!("vBazQux"), BindingTypes::SetupMaybeRef),
        ]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        macro_rules! assert_resolved {
            ($atom: literal) => {{
                let v = fervid_atom!($atom);
                template_visitor.maybe_resolve_directive(&v);
                assert!(matches!(
                    template_visitor.bindings_helper.custom_directives.get(&v),
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
        let mut bindings_helper = with_bindings(vec![
            SetupBinding(fervid_atom!("Foo"), BindingTypes::Imported),
            SetupBinding(fervid_atom!("bar"), BindingTypes::SetupLet),
            SetupBinding(fervid_atom!("bazQux"), BindingTypes::SetupMaybeRef),
            SetupBinding(fervid_atom!("TestNotDirective"), BindingTypes::SetupConst),
        ]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        macro_rules! assert_unresolved {
            ($atom: literal) => {{
                let v = fervid_atom!($atom);
                template_visitor.maybe_resolve_directive(&v);
                assert!(matches!(
                    template_visitor.bindings_helper.custom_directives.get(&v),
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
        let mut bindings_helper = with_bindings(vec![
            SetupBinding(fervid_atom!("ChildComp"), BindingTypes::Component),
            SetupBinding(fervid_atom!("SomeOtherComp"), BindingTypes::Component),
            SetupBinding(fervid_atom!("vMyDir"), BindingTypes::Imported),
        ]);
        let mut template_visitor = from_helper(&mut bindings_helper);

        // <div v-my-dir></div>
        let v_my_dir = fervid_atom!("my-dir");
        template_visitor.maybe_resolve_directive(&v_my_dir);
        assert!(matches!(
            template_visitor
                .bindings_helper
                .custom_directives
                .get(&v_my_dir),
            Some(CustomDirectiveBinding::Resolved(_))
        ));

        // <ChildComp/>
        let child_comp = fervid_atom!("ChildComp");
        template_visitor.maybe_resolve_component(&child_comp);
        assert!(matches!(
            template_visitor.bindings_helper.components.get(&child_comp),
            Some(ComponentBinding::Resolved(_))
        ));

        // <some-other-comp/>
        let some_other_comp = fervid_atom!("some-other-comp");
        template_visitor.maybe_resolve_component(&some_other_comp);
        assert!(matches!(
            template_visitor
                .bindings_helper
                .components
                .get(&some_other_comp),
            Some(ComponentBinding::Resolved(_))
        ));
    }

    fn with_bindings(mut bindings: Vec<SetupBinding>) -> BindingsHelper {
        let mut bindings_helper = BindingsHelper::default();
        bindings_helper.setup_bindings.append(&mut bindings);
        bindings_helper
    }

    fn from_helper<'h>(bindings_helper: &'h mut BindingsHelper) -> TemplateVisitor<'h> {
        TemplateVisitor {
            bindings_helper,
            current_scope: 0,
            v_for_scope: false,
        }
    }
}
