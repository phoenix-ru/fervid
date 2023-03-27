use std::time::Instant;

static JS_BUILTINS: [&str; 7] = ["true", "false", "null", "undefined", "Array", "Set", "Map"];

pub struct Scope<'a> {
    variables: Vec<&'a str>,
    parent: u32
}

#[derive(Default)]
pub struct ScopeHelper<'a> {
    pub template_scopes: Vec<Scope<'a>>,
    setup_vars: Vec<&'a str>,
    props_vars: Vec<&'a str>,
    data_vars: Vec<&'a str>,
    options_vars: Vec<&'a str>,
    globals: Vec<&'a str>
}

#[derive(Debug, PartialEq)]
pub enum VarScopeDescriptor {
    Builtin,
    Data,
    Global,
    Props,
    Options,
    Setup,
    Template(u32), // I know it takes 4 extra bytes, but this is more convenient
    Unknown
}

impl <'a> ScopeHelper<'a> {
    pub fn add_template_scope(&mut self, scope: Scope<'a>) {
        self.template_scopes.push(scope)
    }

    pub fn add_template_scopes(&mut self, mut scopes: Vec<Scope<'a>>) {
        self.template_scopes.append(&mut scopes)
    }

    pub fn find_scope_of_variable(&self, starting_scope: u32, variable: &str) -> VarScopeDescriptor {
        let mut current_scope_index = starting_scope;

        // Macro to check if the variable is in the slice/Vec and conditionally return
        macro_rules! check_scope {
            ($vars: expr, $ret_descriptor: expr) => {
                if $vars.iter().any(|it| *it == variable) {
                    return $ret_descriptor;
                }
            };
        }

        // Check builtins and globals
        check_scope!(JS_BUILTINS, VarScopeDescriptor::Builtin);
        check_scope!(self.globals, VarScopeDescriptor::Global);

        // Check template scope
        while let Some(current_scope) = self.template_scopes.get(current_scope_index as usize) {
            // Check variable existence in the current scope
            let found = current_scope.variables.iter().find(|it| **it == variable);
            if let Some(_) = found {
                return VarScopeDescriptor::Template(current_scope_index);
            }

            // Check if we reached the root scope, it will have itself as a parent
            if current_scope.parent == current_scope_index {
                break;
            }

            // Go to parent
            current_scope_index = current_scope.parent;
        }

        // Check setup vars, props, data and options
        check_scope!(self.setup_vars, VarScopeDescriptor::Setup);
        check_scope!(self.props_vars, VarScopeDescriptor::Props);
        check_scope!(self.data_vars, VarScopeDescriptor::Data);
        check_scope!(self.options_vars, VarScopeDescriptor::Options);

        VarScopeDescriptor::Unknown
    }
}

#[test]
fn feature() {
    let root_scope = Scope {
        parent: 0,
        variables: vec!["root"],
    };

    let child_scope = Scope {
        parent: 0,
        variables: vec!["child1", "child2"],
    };

    let grandchild1_scope = Scope {
        parent: 1,
        variables: vec!["grand1_1", "grand1_2"],
    };

    let grandchild2_scope = Scope {
        parent: 1,
        variables: vec!["grand2_1", "grand2_2"],
    };

    let mut scope_helper = ScopeHelper::default();
    scope_helper.add_template_scopes(
        vec![root_scope, child_scope, grandchild1_scope, grandchild2_scope]
    );

    // Measure time to get an idea on performance
    // TODO move this to Criterion
    let st = Instant::now();

    // All scopes have a root variable
    assert_eq!(scope_helper.find_scope_of_variable(0, "root"), VarScopeDescriptor::Template(0));
    assert_eq!(scope_helper.find_scope_of_variable(1, "root"), VarScopeDescriptor::Template(0));
    assert_eq!(scope_helper.find_scope_of_variable(2, "root"), VarScopeDescriptor::Template(0));
    assert_eq!(scope_helper.find_scope_of_variable(3, "root"), VarScopeDescriptor::Template(0));
    println!("Elapsed root: {:?}", st.elapsed());

    // Only `child1` and its children have `child1` and `child2` vars
    assert_eq!(scope_helper.find_scope_of_variable(0, "child1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(1, "child1"), VarScopeDescriptor::Template(1));
    assert_eq!(scope_helper.find_scope_of_variable(2, "child1"), VarScopeDescriptor::Template(1));
    assert_eq!(scope_helper.find_scope_of_variable(3, "child1"), VarScopeDescriptor::Template(1));
    assert_eq!(scope_helper.find_scope_of_variable(0, "child2"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(1, "child2"), VarScopeDescriptor::Template(1));
    assert_eq!(scope_helper.find_scope_of_variable(2, "child2"), VarScopeDescriptor::Template(1));
    assert_eq!(scope_helper.find_scope_of_variable(3, "child2"), VarScopeDescriptor::Template(1));
    println!("Elapsed child1: {:?}", st.elapsed());

    // Only `grandchild1` has `grand1_1` and `grand1_2` vars
    assert_eq!(scope_helper.find_scope_of_variable(0, "grand1_1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(1, "grand1_1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(2, "grand1_1"), VarScopeDescriptor::Template(2));
    assert_eq!(scope_helper.find_scope_of_variable(3, "grand1_1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(0, "grand1_2"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(1, "grand1_2"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(2, "grand1_2"), VarScopeDescriptor::Template(2));
    assert_eq!(scope_helper.find_scope_of_variable(3, "grand1_2"), VarScopeDescriptor::Unknown);
    println!("Elapsed grand1: {:?}", st.elapsed());

    // Only `grandchild2` has `grand2_1` and `grand2_2` vars
    assert_eq!(scope_helper.find_scope_of_variable(0, "grand2_1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(1, "grand2_1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(2, "grand2_1"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(3, "grand2_1"), VarScopeDescriptor::Template(3));
    assert_eq!(scope_helper.find_scope_of_variable(0, "grand2_2"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(1, "grand2_2"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(2, "grand2_2"), VarScopeDescriptor::Unknown);
    assert_eq!(scope_helper.find_scope_of_variable(3, "grand2_2"), VarScopeDescriptor::Template(3));

    println!("Elapsed total: {:?}", st.elapsed())
}
