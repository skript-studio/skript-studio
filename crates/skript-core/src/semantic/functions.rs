use std::collections::HashMap;

use crate::ast::{
    Expression, FunctionDecl, Program, Span, TopLevelItem, Visitor,
    walk_expression, walk_top_level_item, walk_program,
};

/// A registered function declaration with metadata about its calls.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub decl: FunctionDecl,
    pub parameter_count: usize,
    pub has_return_type: bool,
    pub calls: Vec<FunctionCall>,
}

/// A call to a function with its argument count and source location.
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub argument_count: usize,
    pub call_span: Span,
}

/// An error related to function declaration or usage.
#[derive(Debug, Clone)]
pub struct FunctionError {
    pub message: String,
    pub span: Span,
}

/// Registry of all function declarations and calls in a program.
#[derive(Debug, Clone)]
pub struct FunctionRegistry {
    pub functions: HashMap<String, FunctionInfo>,
    pub calls: Vec<FunctionCall>,
    pub errors: Vec<String>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        FunctionRegistry {
            functions: HashMap::new(),
            calls: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Register a function declaration.
    ///
    /// Returns `Err` if a function with the same name is already registered.
    pub fn register(&mut self, decl: &FunctionDecl) -> Result<(), String> {
        if self.functions.contains_key(&decl.name) {
            return Err(format!("Duplicate function '{}'", decl.name));
        }
        let info = FunctionInfo {
            name: decl.name.clone(),
            decl: decl.clone(),
            parameter_count: decl.parameters.len(),
            has_return_type: decl.return_type.is_some(),
            calls: Vec::new(),
        };
        self.functions.insert(decl.name.clone(), info);
        Ok(())
    }

    /// Record a function call site.
    pub fn record_call(&mut self, name: &str, argument_count: usize, call_span: Span) {
        let call = FunctionCall {
            name: name.to_string(),
            argument_count,
            call_span,
        };
        self.calls.push(call.clone());

        if let Some(info) = self.functions.get_mut(name) {
            info.calls.push(call);
        }
    }

    /// Validate all calls against their declarations.
    ///
    /// Returns a list of errors for calls with wrong argument counts.
    pub fn validate_calls(&self) -> Vec<FunctionError> {
        let mut errors = Vec::new();
        for call in &self.calls {
            if let Some(info) = self.functions.get(&call.name) {
                if call.argument_count != info.parameter_count {
                    errors.push(FunctionError {
                        message: format!(
                            "Function '{}' expects {} arguments but got {}",
                            call.name, info.parameter_count, call.argument_count
                        ),
                        span: call.call_span,
                    });
                }
            } else {
                errors.push(FunctionError {
                    message: format!("Call to undeclared function '{}'", call.name),
                    span: call.call_span,
                });
            }
        }
        errors
    }

    /// Check for duplicate function declarations.
    ///
    /// This returns errors for declarations that were rejected during
    /// registration. A registry built with [`FunctionRegistryBuilder`] will
    /// always have exactly one entry per name (the first encountered).
    pub fn check_duplicates(&self) -> Vec<FunctionError> {
        self.errors
            .iter()
            .map(|msg| FunctionError {
                message: msg.clone(),
                span: Span::new(0, 0),
            })
            .collect()
    }

    /// Get info about a declared function by name.
    pub fn get(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.get(name)
    }

    /// Iterate over all registered functions.
    pub fn functions(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.values()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a [`FunctionRegistry`] by walking a [`Program`] AST.
pub struct FunctionRegistryBuilder {
    pub registry: FunctionRegistry,
}

impl FunctionRegistryBuilder {
    pub fn new() -> Self {
        FunctionRegistryBuilder {
            registry: FunctionRegistry::new(),
        }
    }

    /// Walk the given `program` and produce the final [`FunctionRegistry`].
    pub fn build(mut self, program: &Program) -> FunctionRegistry {
        self.visit_program(program);
        self.registry
    }
}

impl Default for FunctionRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor for FunctionRegistryBuilder {
    fn visit_program(&mut self, program: &Program) {
        walk_program(self, program);
    }

    fn visit_top_level_item(&mut self, item: &TopLevelItem) {
        match item {
            TopLevelItem::Function(f) => {
                if let Err(e) = self.registry.register(f) {
                    self.registry.errors.push(e);
                }
                walk_top_level_item(self, item);
            }
            _ => walk_top_level_item(self, item),
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::FunctionCall(call) => {
                if let Expression::Variable(v) = call.callee.as_ref() {
                    self.registry
                        .record_call(&v.name, call.arguments.len(), call.span);
                }
                for arg in &call.arguments {
                    self.visit_expression(arg);
                }
            }
            _ => walk_expression(self, expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_program;

    #[test]
    fn test_register_function() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let info = registry.get("greet").expect("function should be registered");
        assert_eq!(info.name, "greet");
        assert_eq!(info.parameter_count, 1);
        assert!(!info.has_return_type);
    }

    #[test]
    fn test_duplicate_function_error() {
        let source = "function a():\n\treturn\nfunction a():\n\treturn";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert!(!registry.errors.is_empty(), "duplicate should produce error");
        let dups = registry.check_duplicates();
        assert!(!dups.is_empty());
        assert!(dups[0].message.contains("Duplicate"));
    }

    #[test]
    fn test_record_call() {
        let source = "function test():\n\treturn foo()";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert_eq!(registry.calls.len(), 1);
        assert_eq!(registry.calls[0].name, "foo");
        assert_eq!(registry.calls[0].argument_count, 0);
    }

    #[test]
    fn test_validate_call_correct() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        let mut registry = FunctionRegistryBuilder::new().build(&result.program);

        registry.record_call("greet", 1, Span::new(0, 0));
        let errors = registry.validate_calls();
        let undeclared: Vec<_> = errors.iter().filter(|e| e.message.contains("undeclared")).collect();
        assert!(undeclared.is_empty(), "greet is declared, call with 1 arg should be valid");
    }

    #[test]
    fn test_validate_call_wrong_args() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        let mut registry = FunctionRegistryBuilder::new().build(&result.program);

        registry.record_call("greet", 3, Span::new(0, 1));
        let errors = registry.validate_calls();
        let arg_errors: Vec<_> = errors.iter().filter(|e| e.message.contains("expects")).collect();
        assert_eq!(arg_errors.len(), 1);
        assert!(arg_errors[0].message.contains("expects 1 arguments but got 3"));
    }

    #[test]
    fn test_undeclared_function_call() {
        let source = "function test():\n\treturn foo()";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let errors = registry.validate_calls();
        assert!(errors.iter().any(|e| e.message.contains("undeclared")));
    }

    #[test]
    fn test_get_function_info() {
        let source = "function add(a: number, b: number) :: number:\n\treturn 0";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let info = registry.get("add").expect("function should exist");
        assert_eq!(info.name, "add");
        assert_eq!(info.parameter_count, 2);
        assert!(info.has_return_type);
    }

    #[test]
    fn test_multiple_functions() {
        let source = "function a():\n\treturn\nfunction b():\n\treturn";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert_eq!(registry.functions.len(), 2);
        assert!(registry.get("a").is_some());
        assert!(registry.get("b").is_some());
    }

    #[test]
    fn test_function_with_return_type() {
        let source = "function add(a: number, b: number) :: number:\n\treturn a + b";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let info = registry.get("add").expect("function should exist");
        assert!(info.has_return_type);
    }

    #[test]
    fn test_empty_registry() {
        let source = "";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert!(registry.functions.is_empty());
        assert!(registry.calls.is_empty());
        assert!(registry.errors.is_empty());
    }

    #[test]
    fn test_record_call_from_builder() {
        let source = "function test():\n\treturn bar(1, 2)";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert_eq!(registry.calls.len(), 1);
        assert_eq!(registry.calls[0].name, "bar");
        assert_eq!(registry.calls[0].argument_count, 2);
    }

    #[test]
    fn test_nested_call_inside_expression() {
        let source = "function test():\n\tif bar():\n\t\tsend \"hi\"";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let bar_calls: Vec<_> = registry.calls.iter().filter(|c| c.name == "bar").collect();
        assert_eq!(bar_calls.len(), 1);
    }

    #[test]
    fn test_multiple_calls_to_same_function() {
        let source = "function test():\n\treturn foo(1) + foo(2)";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let foo_calls: Vec<_> = registry.calls.iter().filter(|c| c.name == "foo").collect();
        assert_eq!(foo_calls.len(), 2);
    }

    #[test]
    fn test_function_calls_inside_function_body() {
        let source = "function caller():\n\treturn callee(42)";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert!(registry.get("caller").is_some());
        assert_eq!(registry.calls.len(), 1);
        assert_eq!(registry.calls[0].name, "callee");
        assert_eq!(registry.calls[0].argument_count, 1);
    }

    #[test]
    fn test_register_preserves_first_declaration() {
        let source = "function dup():\n\treturn\nfunction dup():\n\treturn";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        assert_eq!(registry.functions.len(), 1);
        assert!(registry.get("dup").is_some());
        assert_eq!(registry.errors.len(), 1);
    }

    #[test]
    fn test_validate_call_no_declared_functions() {
        let source = "function test():\n\treturn foo() + bar(1, 2)";
        let result = parse_program(source);
        let registry = FunctionRegistryBuilder::new().build(&result.program);

        let errors = registry.validate_calls();
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|e| e.message.contains("undeclared")));
    }

    #[test]
    fn test_declared_function_call_site_tracking() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        let mut registry = FunctionRegistryBuilder::new().build(&result.program);

        registry.record_call("greet", 1, Span::new(5, 10));
        let info = registry.get("greet").unwrap();
        assert_eq!(info.calls.len(), 1);
        assert_eq!(info.calls[0].call_span.start, 5);
        assert_eq!(info.calls[0].call_span.end, 10);
    }
}
