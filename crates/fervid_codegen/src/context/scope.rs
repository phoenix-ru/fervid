use fervid_core::BindingTypes;
use fervid_script::structs::{ScriptLegacyVars, SetupBinding};
use smallvec::SmallVec;
use swc_core::ecma::atoms::JsWord;

use super::js_builtins::JS_BUILTINS;

#[derive(Debug)]
pub struct TemplateScope {
    variables: SmallVec<[JsWord; 1]>,
    parent: u32,
}

#[derive(Debug, Default)]
pub struct ScopeHelper {
    pub template_scopes: Vec<TemplateScope>,
    pub setup_bindings: Vec<SetupBinding>,
    pub options_api_vars: Box<ScriptLegacyVars>,
    pub is_inline: bool,
}

impl ScopeHelper {
    pub fn get_var_binding_type(&self, starting_scope: u32, variable: &str) -> BindingTypes {
        if JS_BUILTINS.contains(variable) {
            return BindingTypes::JsGlobal;
        }

        let mut current_scope_index = starting_scope;

        // Check template scope
        while let Some(current_scope) = self.template_scopes.get(current_scope_index as usize) {
            // Check variable existence in the current scope
            let found = current_scope.variables.iter().find(|it| *it == variable);

            if let Some(_) = found {
                return BindingTypes::TemplateLocal;
            }

            // Check if we reached the root scope, it will have itself as a parent
            if current_scope.parent == current_scope_index {
                break;
            }

            // Go to parent
            current_scope_index = current_scope.parent;
        }

        // Check setup bindings (both `<script setup>` and `setup()`)
        for binding in self
            .setup_bindings
            .iter()
            .chain(self.options_api_vars.setup.iter())
        {
            if &binding.0 == variable {
                return binding.1;
            }
        }

        // Macro to check if the variable is in the slice/Vec and conditionally return
        macro_rules! check_scope {
            ($vars: expr, $ret_descriptor: expr) => {
                if $vars.iter().any(|it| it == variable) {
                    return $ret_descriptor;
                }
            };
        }

        // Check all the options API variables
        let options_api_vars = &self.options_api_vars;
        check_scope!(options_api_vars.data, BindingTypes::Data);
        check_scope!(options_api_vars.props, BindingTypes::Props);
        check_scope!(options_api_vars.computed, BindingTypes::Options);
        check_scope!(options_api_vars.methods, BindingTypes::Options);
        check_scope!(options_api_vars.inject, BindingTypes::Options);

        // Check options API imports.
        // Currently it ignores the SyntaxContext (same as in js implementation)
        for binding in options_api_vars.imports.iter() {
            if &binding.0 == variable {
                return BindingTypes::SetupMaybeRef;
            }
        }

        BindingTypes::Unresolved
    }
}

/// Gets the variable prefix depending on if we are compiling the template in inline mode.
/// This is used for transformations.
/// ## Example
/// `data()` variable `foo` in non-inline compilation becomes `$data.foo`.\
/// `setup()` ref variable `bar` in non-inline compilation becomes `$setup.bar`,
/// but in the inline compilation it remains the same.
pub fn get_prefix(binding_type: &BindingTypes, is_inline: bool) -> Option<JsWord> {
    // For inline mode, options API variables become prefixed
    if is_inline {
        return match binding_type {
            BindingTypes::Data | BindingTypes::Options => Some(JsWord::from("_ctx")),
            BindingTypes::Props => Some(JsWord::from("__props")),
            // TODO This is not correct. The transform implementation must handle `unref`
            _ => None,
        };
    }

    match binding_type {
        BindingTypes::Data => Some(JsWord::from("$data")),
        BindingTypes::Props => Some(JsWord::from("$props")),
        BindingTypes::Options => Some(JsWord::from("$options")),
        BindingTypes::TemplateLocal | BindingTypes::JsGlobal | BindingTypes::LiteralConst => None,
        BindingTypes::SetupConst
        | BindingTypes::SetupLet
        | BindingTypes::SetupMaybeRef
        | BindingTypes::SetupReactiveConst
        | BindingTypes::SetupRef => Some(JsWord::from("$setup")),
        BindingTypes::Unresolved => Some(JsWord::from("_ctx")),
        BindingTypes::PropsAliased => unimplemented!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_acknowledges_builtins() {
        let mut helper = ScopeHelper::default();

        for builtin in JS_BUILTINS.iter() {
            assert_eq!(
                BindingTypes::JsGlobal,
                helper.get_var_binding_type(0, builtin)
            );
        }

        // Check inline mode as well
        helper.is_inline = true;
        for builtin in JS_BUILTINS.iter() {
            assert_eq!(
                BindingTypes::JsGlobal,
                helper.get_var_binding_type(0, builtin)
            );
        }
    }
}
