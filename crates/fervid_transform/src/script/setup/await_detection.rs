use swc_core::ecma::{
    ast::{Expr, ModuleItem},
    visit::{Visit, VisitWith},
};

/// Detects usage of "await" inside expressions
pub fn detect_await_module_item(module_item: &ModuleItem) -> bool {
    let mut await_detector = AwaitDetector::default();
    module_item.visit_with(&mut await_detector);
    await_detector.found
}

#[derive(Default)]
struct AwaitDetector {
    found: bool,
}

impl Visit for AwaitDetector {
    fn visit_await_expr(&mut self, _: &swc_core::ecma::ast::AwaitExpr) {
        self.found = true;
    }

    fn visit_function(&mut self, n: &swc_core::ecma::ast::Function) {
        for param in n.params.iter() {
            if self.found {
                return;
            }

            param.visit_with(self);
        }
    }

    fn visit_expr(&mut self, n: &Expr) {
        if self.found {
            return;
        }

        n.visit_children_with(self);
    }

    fn visit_arrow_expr(&mut self, n: &swc_core::ecma::ast::ArrowExpr) {
        for param in n.params.iter() {
            if self.found {
                return;
            }

            param.visit_with(self);
        }
    }
}

#[cfg(test)]
mod tests {
    //! https://github.com/vuejs/core/blob/46c2b63981b8321be2d8bb1892b74d7e50bdd668/packages/compiler-sfc/__tests__/compileScript.spec.ts#L748-L860
    use crate::test_utils::parser::parse_typescript_module;

    use super::*;

    macro_rules! assert_await_detection {
        ($code: literal) => {
            assert_await_detection_fn($code, true);
        };
        ($code: literal, false) => {
            assert_await_detection_fn($code, false);
        };
    }

    #[test]
    fn expression_statement() {
        assert_await_detection!("await foo");
    }

    #[test]
    fn variable() {
        assert_await_detection!("const a = 1 + (await foo)");
    }

    #[test]
    fn ref_() {
        assert_await_detection!("let a = ref(1 + (await foo))");
    }

    #[test]
    fn nested_await() {
        assert_await_detection!("await (await foo)");
        assert_await_detection!("await ((await foo))");
        assert_await_detection!("await (await (await foo))");
    }

    #[test]
    fn nested_leading_await_in_expression_statement() {
        assert_await_detection!("foo()\nawait 1 + await 2");
    }

    #[test]
    fn single_line_conditions() {
        assert_await_detection!("if (false) await foo()");
    }

    #[test]
    fn nested_statements() {
        assert_await_detection!("if (ok) { await foo } else { await bar }");
    }

    #[test]
    fn multiple_if_nested_statements() {
        assert_await_detection!(
            "if (ok) {
          let a = 'foo'
          await 0 + await 1
          await 2
        } else if (a) {
          await 10
          if (b) {
            await 0 + await 1
          } else {
            let a = 'foo'
            await 2
          }
          if (b) {
            await 3
            await 4
          }
        } else {
          await 5
        }"
        );
    }

    #[test]
    fn multiple_if_while_nested_statements() {
        assert_await_detection!(
            "if (ok) {
          while (d) {
            await 5
          }
          while (d) {
            await 5
            await 6
            if (c) {
              let f = 10
              10 + await 7
            } else {
              await 8
              await 9
            }
          }
        }"
        );
    }

    #[test]
    fn multiple_if_for_nested_statements() {
        assert_await_detection!(
            "if (ok) {
          for (let a of [1,2,3]) {
            await a
          }
          for (let a of [1,2,3]) {
            await a
            await a
          }
        }"
        );
    }

    #[test]
    fn should_ignore_await_inside_functions() {
        // function declaration
        assert_await_detection!("async function foo() { await bar }", false);
        // function expression
        assert_await_detection!("const foo = async () => { await bar }", false);
        // object method
        assert_await_detection!("const obj = { async method() { await bar }}", false);
        // class method
        assert_await_detection!(
            "const cls = class Foo { async method() { await bar }}",
            false
        );
    }

    fn assert_await_detection_fn(input: &str, should_async: bool) {
        let code = parse_typescript_module(input, 0, Default::default())
            .expect("Should be parseable")
            .0;

        let found = code
            .body
            .iter()
            .any(|module_item| detect_await_module_item(module_item));
        assert_eq!(should_async, found);
    }
}
