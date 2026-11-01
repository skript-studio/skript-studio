pub mod functions;
pub mod scope;
pub mod types;
pub mod variables;

use crate::ast::Program;
use crate::parser::parse_program;
pub use functions::FunctionRegistry;
pub use scope::ScopeTree;
pub use types::TypeInference;
pub use variables::VariableTracker;

/// The result of full semantic analysis of a Skript program.
#[derive(Debug)]
pub struct SemanticAnalysis {
    pub scope_tree: ScopeTree,
    pub function_registry: FunctionRegistry,
    pub variable_tracker: VariableTracker,
    pub type_inference: TypeInference,
    pub errors: Vec<String>,
}

/// Run full semantic analysis on a parsed Skript program.
pub fn analyze(program: &Program) -> SemanticAnalysis {
    let scope_tree = scope::ScopeTreeBuilder::new().build(program);
    let function_registry = functions::FunctionRegistryBuilder::new().build(program);
    let variable_tracker = variables::VariableTrackerBuilder::new().build(program);
    let type_inference =
        types::TypeInferenceBuilder::new(scope_tree.clone(), function_registry.clone())
            .build(program);

    let mut errors: Vec<String> = Vec::new();
    errors.extend(function_registry.errors.iter().cloned());
    for err in &variable_tracker.errors {
        errors.push(err.message.clone());
    }
    errors.extend(type_inference.errors.iter().cloned());

    for err in function_registry.validate_calls() {
        errors.push(err.message);
    }
    for err in function_registry.check_duplicates() {
        errors.push(err.message);
    }

    SemanticAnalysis {
        scope_tree,
        function_registry,
        variable_tracker,
        type_inference,
        errors,
    }
}

/// Convenience: parse and analyze a Skript source string in one call.
pub fn analyze_source(source: &str) -> SemanticAnalysis {
    let result = parse_program(source);
    let mut analysis = analyze(&result.program);
    for err in result.errors {
        analysis.errors.push(err.message);
    }
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_empty() {
        let result = analyze_source("");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_analyze_function() {
        let result = analyze_source("function greet(name: text):\n\tbroadcast \"Hello\"");
        assert!(result.errors.is_empty());
        assert!(!result.function_registry.functions.is_empty());
    }

    #[test]
    fn test_analyze_variable_usage() {
        let result = analyze_source("function test():\n\tset {_x} to 5\n\tbroadcast {_x}");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_analyze_unused_variable() {
        let result = analyze_source("function test():\n\tset {_x} to 5");
        assert!(!result.errors.is_empty(), "expected errors, got none");
    }

    #[test]
    fn test_analyze_duplicate_function() {
        let result = analyze_source(
            "function foo():\n\tbroadcast \"a\"\nfunction foo():\n\tbroadcast \"b\"",
        );
        assert!(
            !result.errors.is_empty(),
            "expected errors, got: {:#?}",
            result.errors
        );
    }
}
