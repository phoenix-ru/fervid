use lazy_static::lazy_static;
use regex::Regex;
use swc_common::BytePos;
use swc_core::ecma::{visit::{Visit, VisitWith}, atoms::JsWord};
use swc_ecma_parser::{lexer::Lexer, Syntax, StringInput, Parser};

use crate::parser::{structs::Node, attributes::{HtmlAttribute, VDirective}};

lazy_static! {
    static ref JS_BUILTINS: [JsWord; 7] = ["true", "false", "null", "undefined", "Array", "Set", "Map"].map(JsWord::from);
}

// Regex for v-for directive
lazy_static! {
    static ref FOR_RE: Regex = Regex::new(r"^(.+)\s+(in|of)\s+(.+)$").unwrap();
}

#[derive(Debug)]
pub struct Scope {
    variables: Vec<JsWord>,
    parent: u32
}

#[derive(Default, Debug)]
pub struct ScopeHelper {
    pub template_scopes: Vec<Scope>,
    setup_vars: Vec<JsWord>,
    props_vars: Vec<JsWord>,
    data_vars: Vec<JsWord>,
    options_vars: Vec<JsWord>,
    globals: Vec<JsWord>
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

impl VarScopeDescriptor {
    pub fn get_prefix(&self) -> &'static str {
        match self {
            Self::Builtin => "",
            Self::Data => "$data.",
            Self::Global => "_ctx.",
            Self::Props => "$props.",
            Self::Options => "$options.",
            Self::Setup => "$setup.",
            Self::Template(_) => "",
            Self::Unknown => "_ctx."
        }
    }
}

impl ScopeHelper {
    pub fn add_template_scope(&mut self, scope: Scope) {
        self.template_scopes.push(scope)
    }

    pub fn add_template_scopes(&mut self, mut scopes: Vec<Scope>) {
        self.template_scopes.append(&mut scopes)
    }

    pub fn find_scope_of_variable(&self, starting_scope: u32, variable: &str) -> VarScopeDescriptor {
        let mut current_scope_index = starting_scope;
        let variable = JsWord::from(variable);

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

    /// Transforms an AST by assigning the scope identifiers to Nodes
    /// The variables introduced in `v-for` and `v-slot` are recorded to the ScopeHelper
    pub fn transform_and_record_ast(&mut self, ast: &mut [Node]) {
        // Pre-allocate template scopes to at least the amount of root AST nodes
        if self.template_scopes.len() == 0 && ast.len() != 0 {
            self.template_scopes.reserve(ast.len());

            // Add scope 0.
            // It may be left unused, as it's reserved for some global template vars (undecided)
            self.template_scopes.push(Scope { variables: vec![], parent: 0 });
        }

        for node in ast {
            self.walk_ast_node(node, 0)
        }
    }

    fn walk_ast_node(&mut self, node: &mut Node, current_scope_identifier: u32) {
        match node {
            Node::ElementNode(element_node) => {
                // Finds a `v-for` or `v-slot` directive when in ElementNode
                let scoping_directive = element_node.starting_tag
                    .attributes
                    .iter()
                    .find_map(|attr| match attr {
                        HtmlAttribute::VDirective(directive)
                        if (directive.name == "for" || directive.name == "slot") &&
                        directive.value.is_some() => Some(directive),
            
                        _ => None
                    });

                // A scope to use for both the current node and its children (as a parent)
                let mut scope_to_use = current_scope_identifier;

                // Create a new scope
                if let Some(directive) = scoping_directive {
                    // New scope will have ID equal to length
                    scope_to_use = self.template_scopes.len() as u32;
                    self.template_scopes.push(Scope {
                        variables: vec![],
                        parent: current_scope_identifier
                    });

                    // TODO Fail somehow if directive value is invalid?

                    self.extract_directive_variables(directive, scope_to_use);
                }

                // Update Node's scope
                element_node.template_scope = scope_to_use;

                // Walk children
                for mut child in element_node.children.iter_mut() {
                    self.walk_ast_node(&mut child, scope_to_use);
                }
            },

            // For dynamic expression, just update the scope
            Node::DynamicExpression { template_scope, .. } => {
                *template_scope = current_scope_identifier;
            },

            _ => {}
        }
    }

    /// Extracts the variables introduced by `v-for` or `v-slot`
    fn extract_directive_variables(&mut self, directive: &VDirective, scope_to_use: u32) {
        match (directive.value, directive.name) {
            (Some(directive_value), "for") => {
                // TODO if there are no matches, what do we do?
                let Some(captures) = FOR_RE.captures(directive_value) else {
                    return;
                };

                // We only care of the left hand side variables
                let introduced_variables = captures.get(1).map_or("", |x| x.as_str().trim());

                // Get the needed scope and collect variables to it
                let mut scope = &mut self.template_scopes[scope_to_use as usize];
                Self::collect_variables(introduced_variables, &mut scope);
            },

            (Some(directive_value), "slot") => {
                // Get the needed scope and collect variables to it
                let mut scope = &mut self.template_scopes[scope_to_use as usize];
                Self::collect_variables(directive_value, &mut scope);
            },

            _ => {}
        }
    }

    fn collect_variables(input: &str, scope: &mut Scope) {
        let lexer = Lexer::new(
            // We want to parse ecmascript
            Syntax::Es(Default::default()),
            // EsVersion defaults to es5
            Default::default(),
            StringInput::new(input, BytePos(0), BytePos(1000)),
            None,
        );

        let mut parser = Parser::new_from(lexer);

        match parser.parse_expr() {
            Ok(expr) => {
                let mut visitor = IdentifierVisitor {
                    collected: vec![]
                };

                expr.visit_with(&mut visitor);

                scope.variables.reserve(visitor.collected.len());
                for collected in visitor.collected {
                    scope.variables.push(collected.sym)
                }
            },

            _ => {}
        }
    }
}

struct IdentifierVisitor {
    collected: Vec<swc_core::ecma::ast::Ident>
}

impl Visit for IdentifierVisitor {
    fn visit_ident(&mut self, n: &swc_core::ecma::ast::Ident) {
        self.collected.push(n.to_owned());
    }

    fn visit_object_lit(&mut self, n: &swc_core::ecma::ast::ObjectLit) {
        self.collected.reserve(n.props.len());

        for prop in n.props.iter() {
            let swc_core::ecma::ast::PropOrSpread::Prop(prop) = prop else {
                continue;
            };

            // This is shorthand `a` in `{ a }`
            let shorthand = prop.as_shorthand();
            if let Some(ident) = shorthand {
                self.collected.push(ident.to_owned());
                continue;
            }

            // This is key-value `a: b` in `{ a: b }`
            let Some(keyvalue) = prop.as_key_value() else { continue };

            // We only support renaming things (therefore value must be an identifier)
            let Some(value) = keyvalue.value.as_ident() else { continue };
            self.collected.push(value.to_owned());
        }
    }
}

#[test]
fn feature() {
    let root_scope = Scope {
        parent: 0,
        variables: vec!["root".into()],
    };

    let child_scope = Scope {
        parent: 0,
        variables: vec!["child1".into(), "child2".into()],
    };

    let grandchild1_scope = Scope {
        parent: 1,
        variables: vec!["grand1_1".into(), "grand1_2".into()],
    };

    let grandchild2_scope = Scope {
        parent: 1,
        variables: vec!["grand2_1".into(), "grand2_2".into()],
    };

    let mut scope_helper = ScopeHelper::default();
    scope_helper.add_template_scopes(
        vec![root_scope, child_scope, grandchild1_scope, grandchild2_scope]
    );

    // Measure time to get an idea on performance
    // TODO move this to Criterion
    let st = std::time::Instant::now();

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
