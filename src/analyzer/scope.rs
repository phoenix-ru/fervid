struct Scope<'a> {
    variables: Vec<&'a str>,
    parent: Option<&'a Scope<'a>>,
}

impl<'a> Scope<'a> {
    fn has(self: &Self, variable: &str) -> bool {
        let mut current_scope = Some(self);
        while let Some(scope) = current_scope {
            if scope.variables.iter().any(|v| v == &variable) {
                return true;
            }

            current_scope = scope.parent;
        }

        false
    }
}

#[test]
fn feature() {
    let root_scope = Scope {
        parent: None,
        variables: vec!["root"],
    };

    let child_scope = Scope {
        parent: Some(&root_scope),
        variables: vec!["child1", "child2"],
    };

    let grandchild1_scope = Scope {
        parent: Some(&child_scope),
        variables: vec!["grand1_1", "grand1_2"],
    };

    let grandchild2_scope = Scope {
        parent: Some(&child_scope),
        variables: vec!["grand2_1", "grand2_2"],
    };

    assert!(root_scope.has("root"));
    assert!(child_scope.has("root"));
    assert!(grandchild1_scope.has("root"));
    assert!(grandchild2_scope.has("root"));

    assert!(child_scope.has("child1"));
    assert!(child_scope.has("child2"));
    assert!(grandchild1_scope.has("child1"));
    assert!(grandchild1_scope.has("child2"));
    assert!(grandchild2_scope.has("child1"));
    assert!(grandchild2_scope.has("child2"));

    assert!(grandchild1_scope.has("grand1_1"));
    assert!(grandchild1_scope.has("grand1_2"));
    assert!(!grandchild1_scope.has("grand2_1"));
    assert!(!grandchild1_scope.has("grand2_2"));

    assert!(grandchild2_scope.has("grand2_1"));
    assert!(grandchild2_scope.has("grand2_2"));
    assert!(!grandchild2_scope.has("grand1_1"));
    assert!(!grandchild2_scope.has("grand1_2"));
}
