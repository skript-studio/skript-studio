pub mod functions;
pub mod scope;
pub mod types;
pub mod variables;

use crate::ast::{Program, Span};
use crate::parser::parse_program;
pub use functions::FunctionRegistry;
pub use scope::ScopeTree;
pub use types::TypeInference;
pub use variables::VariableTracker;

/// A semantic diagnostic with its source span and a stable rule code.
///
/// Unlike the previous `Vec<String>` shape, positions survive to the LSP
/// layer (they used to be flattened away, landing every semantic diagnostic
/// on line 1).
#[derive(Debug, Clone)]
pub struct SemanticError {
    pub message: String,
    pub span: Span,
    /// Stable rule identifier, e.g. `"unknown-function"`.
    pub code: &'static str,
}

impl SemanticError {
    pub fn new(message: String, span: Span, code: &'static str) -> Self {
        SemanticError {
            message,
            span,
            code,
        }
    }
}

/// The result of full semantic analysis of a Skript program.
#[derive(Debug)]
pub struct SemanticAnalysis {
    pub scope_tree: ScopeTree,
    pub function_registry: FunctionRegistry,
    pub variable_tracker: VariableTracker,
    pub type_inference: TypeInference,
    pub errors: Vec<SemanticError>,
}

/// Run full semantic analysis on a parsed Skript program.
pub fn analyze(program: &Program) -> SemanticAnalysis {
    let scope_tree = scope::ScopeTreeBuilder::new().build(program);
    let function_registry = functions::FunctionRegistryBuilder::new().build(program);
    let variable_tracker = variables::VariableTrackerBuilder::new().build(program);
    let type_inference =
        types::TypeInferenceBuilder::new(scope_tree.clone(), function_registry.clone())
            .build(program);

    let mut errors: Vec<SemanticError> = Vec::new();
    // Duplicate function declarations (span unknown at this layer).
    for msg in &function_registry.errors {
        errors.push(SemanticError::new(
            msg.clone(),
            Span::new(0, 0),
            "duplicate-function",
        ));
    }
    // Variable errors except "unused local" — that rule is surfaced
    // separately via `variable_tracker.unused_locals()` with real
    // declaration spans (reporting it here as well duplicated every hit).
    for err in &variable_tracker.errors {
        if err.message.starts_with("Unused local variable") {
            continue;
        }
        errors.push(SemanticError::new(
            err.message.clone(),
            err.span,
            "variable",
        ));
    }
    // Type inference errors are string-only today.
    for msg in &type_inference.errors {
        errors.push(SemanticError::new(msg.clone(), Span::new(0, 0), "type"));
    }

    for err in function_registry.validate_calls() {
        errors.push(SemanticError::new(err.message, err.span, "function-call"));
    }
    for err in function_registry.check_duplicates() {
        errors.push(SemanticError::new(
            err.message,
            err.span,
            "duplicate-function",
        ));
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
        analysis
            .errors
            .push(SemanticError::new(err.message, err.span, "parse-error"));
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
        // Unused locals are reported via `unused_locals()` (with spans),
        // NOT via `errors` — that path used to duplicate every warning.
        let result = analyze_source("function test():\n\tset {_x} to 5");
        assert!(
            result.errors.is_empty(),
            "unused locals must not be in errors (they are reported separately), got {:?}",
            result.errors
        );
        assert_eq!(result.variable_tracker.unused_locals().len(), 1);
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

    #[test]
    fn test_builtin_function_not_flagged() {
        let result =
            analyze_source("function f():\n\tset {_loc} to location(1, 2, 3, world \"world\")");
        let unknown: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.message.contains("undeclared"))
            .collect();
        assert!(
            unknown.is_empty(),
            "builtin location() must not be flagged undeclared, got {:?}",
            unknown
        );
    }
}
