//! Utilities for resolving positions to symbols and finding references in the AST.

use crate::ast::{
    AssignmentTarget, Expression, NodeRef, Program, Span, Statement, TopLevelItem, Visitor,
    node_at_offset, walk_expression,
};
use crate::semantic::SemanticAnalysis;

/// The kind of a resolved symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Function,
    Command,
    Event,
    Options,
    Aliases,
    Unknown,
}

/// A resolved symbol with its declaration location.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub declaration_span: Span,
}

/// Resolve the symbol at `offset` in the program.
/// Uses the AST + semantic analysis to find declarations for variable references,
/// function calls, and declaration nodes themselves.
pub fn find_symbol_at_offset(
    program: &Program,
    analysis: &SemanticAnalysis,
    offset: usize,
) -> Option<Symbol> {
    let node = node_at_offset(program, offset)?;
    match node {
        // Variable reference → look up declaration in scope tree
        NodeRef::VariableExpr(expr) => {
            let name = extract_var_name(expr)?;
            let scope = analysis.scope_tree.scope_at_offset(offset);
            let decl = analysis.scope_tree.lookup(scope, name)?;
            Some(Symbol {
                name: name.to_owned(),
                kind: SymbolKind::Variable,
                declaration_span: decl.declared_at,
            })
        }
        // Function call → look up in function registry
        NodeRef::FunctionCall(expr) => {
            let name = extract_func_name(expr)?;
            let func = analysis.function_registry.get(name)?;
            Some(Symbol {
                name: name.to_owned(),
                kind: SymbolKind::Function,
                declaration_span: func.decl.span,
            })
        }
        // Assignment target → variable reference on the LHS of assignment
        NodeRef::Assignment(stmt) => {
            if let Statement::Assignment(a) = stmt {
                if let Some(name) = assignment_target_name(&a.target) {
                    let scope = analysis.scope_tree.scope_at_offset(offset);
                    let decl = analysis.scope_tree.lookup(scope, &name)?;
                    return Some(Symbol {
                        name: name.to_owned(),
                        kind: SymbolKind::Variable,
                        declaration_span: decl.declared_at,
                    });
                }
            }
            None
        }
        // Declaration nodes → self-referencing symbol
        NodeRef::FunctionDecl(item) => {
            if let TopLevelItem::Function(fd) = item {
                Some(Symbol {
                    name: fd.name.clone(),
                    kind: SymbolKind::Function,
                    declaration_span: fd.span,
                })
            } else {
                None
            }
        }
        NodeRef::Command(item) => {
            if let TopLevelItem::Command(cmd) = item {
                Some(Symbol {
                    name: cmd.name.clone(),
                    kind: SymbolKind::Command,
                    declaration_span: cmd.span,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_var_name(expr: &Expression) -> Option<&str> {
    match expr {
        Expression::Variable(v) => Some(v.name.as_str()),
        _ => None,
    }
}

fn extract_func_name(expr: &Expression) -> Option<&str> {
    match expr {
        Expression::FunctionCall(call) => {
            if let Expression::Variable(v) = call.callee.as_ref() {
                Some(v.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn assignment_target_name(target: &AssignmentTarget) -> Option<String> {
    match target {
        AssignmentTarget::Variable(v) => Some(v.name.clone()),
        _ => None,
    }
}

/// Find all references to `symbol` across the program.
/// Returns spans (byte offsets) for callers to convert to LSP locations.
pub fn find_references(program: &Program, symbol: &Symbol) -> Vec<Span> {
    let mut results = Vec::new();
    match symbol.kind {
        SymbolKind::Variable => {
            collect_variable_references(program, &symbol.name, &mut results);
        }
        SymbolKind::Function => {
            collect_function_references(program, &symbol.name, &mut results);
        }
        _ => {}
    }
    results
}

fn collect_variable_references(program: &Program, name: &str, out: &mut Vec<Span>) {
    let mut visitor = VarRefVisitor {
        target: name,
        results: out,
    };
    visitor.visit_program(program);
}

struct VarRefVisitor<'a> {
    target: &'a str,
    results: &'a mut Vec<Span>,
}

impl Visitor for VarRefVisitor<'_> {
    fn visit_assignment_target(&mut self, target: &AssignmentTarget) {
        if let AssignmentTarget::Variable(v) = target {
            if v.name == self.target {
                self.results.push(v.span);
            }
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        if let Some(var_name) = extract_var_name(expr) {
            if var_name == self.target {
                self.results.push(expr.span());
            }
        }
        walk_expression(self, expr);
    }
}

fn collect_function_references(program: &Program, name: &str, out: &mut Vec<Span>) {
    let mut visitor = FuncRefVisitor {
        target: name,
        results: out,
    };
    visitor.visit_program(program);
}

struct FuncRefVisitor<'a> {
    target: &'a str,
    results: &'a mut Vec<Span>,
}

impl Visitor for FuncRefVisitor<'_> {
    fn visit_expression(&mut self, expr: &Expression) {
        if let Some(func_name) = extract_func_name(expr) {
            if func_name == self.target {
                self.results.push(expr.span());
            }
        }
        walk_expression(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_program;
    use crate::semantic::analyze;

    #[test]
    fn test_find_symbol_variable_reference() {
        let source = "function test():\n\tset {_x} to 5\n\tbroadcast {_x}";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.rfind("{_x}").unwrap();
        let symbol = find_symbol_at_offset(&parse_result.program, &analysis, offset);
        assert!(symbol.is_some());
        assert_eq!(symbol.unwrap().kind, SymbolKind::Variable);
    }

    #[test]
    fn test_find_symbol_at_definition() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.find("greet").unwrap();
        let symbol = find_symbol_at_offset(&parse_result.program, &analysis, offset);
        assert!(symbol.is_some());
        assert_eq!(symbol.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_find_symbol_none_for_literal() {
        let source = "broadcast 42";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.find("42").unwrap();
        assert!(find_symbol_at_offset(&parse_result.program, &analysis, offset).is_none());
    }

    #[test]
    fn test_find_references_variable() {
        let source = "function test():\n\tset {_x} to 5\n\tbroadcast {_x}";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.find("{_x}").unwrap();
        let symbol = find_symbol_at_offset(&parse_result.program, &analysis, offset).unwrap();
        let refs = find_references(&parse_result.program, &symbol);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_find_symbol_function_call() {
        let source = "function greet():\n\tbroadcast \"hi\"\nfunction test():\n\tgreet()";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.rfind("greet()").unwrap();
        let symbol = find_symbol_at_offset(&parse_result.program, &analysis, offset);
        assert!(symbol.is_some());
        assert_eq!(symbol.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_find_symbol_command() {
        let source = "command /hello:\n\ttrigger:\n\t\tsend \"hi\"";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.find("/hello").unwrap();
        let symbol = find_symbol_at_offset(&parse_result.program, &analysis, offset);
        assert!(symbol.is_some());
        assert_eq!(symbol.unwrap().kind, SymbolKind::Command);
    }

    #[test]
    fn test_find_references_function() {
        let source = "function greet(name: text):\n\tbroadcast \"hi\"\nfunction test():\n\tbroadcast greet(\"world\")";
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        let offset = source.find("greet(").unwrap();
        let symbol = find_symbol_at_offset(&parse_result.program, &analysis, offset).unwrap();
        let refs = find_references(&parse_result.program, &symbol);
        assert!(!refs.is_empty());
        assert!(refs.iter().any(|s| source[s.start..s.end].contains("greet")));
    }
}
