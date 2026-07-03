use std::collections::HashMap;

use crate::ast::{
    AssignmentTarget, Program, Statement, TopLevelItem, Visitor, Span,
    walk_top_level_item,
};

/// The kind of a scope in the scope tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// The outermost global scope.
    Global,
    /// A function body scope.
    Function,
    /// A command body scope.
    Command,
    /// An event body scope.
    Event,
    /// A block scope (if, while, loop, for-in bodies).
    Block,
}

/// The kind of a declared symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// A local variable.
    Variable,
    /// A function or command parameter.
    Parameter,
    /// A function declaration.
    Function,
    /// A command declaration.
    Command,
    /// An event declaration.
    Event,
    /// A loop variable (for-in).
    LoopVariable,
}

/// A declared symbol in a scope.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    /// The name of the symbol.
    pub name: String,
    /// What kind of symbol this is.
    pub kind: SymbolKind,
    /// Where in the source it was declared.
    pub declared_at: Span,
    /// The declared type, if known.
    pub type_name: Option<String>,
}

/// A single scope node in the scope tree.
#[derive(Debug, Clone)]
pub struct Scope {
    /// What kind of scope this is.
    pub kind: ScopeKind,
    /// Index of the parent scope, or `None` for the root.
    pub parent: Option<usize>,
    /// Indices of child scopes.
    pub children: Vec<usize>,
    /// Symbols declared directly in this scope.
    pub symbols: HashMap<String, Symbol>,
    /// An optional human-readable name for this scope.
    pub name: Option<String>,
}

/// A scope tree for a complete Skript program.
///
/// Scopes are stored arena-style in a `Vec` and referenced by index.
/// The root scope (index `root`) is always the `Global` scope.
#[derive(Debug, Clone)]
pub struct ScopeTree {
    /// All scopes in arena order.
    pub scopes: Vec<Scope>,
    /// Index of the root (global) scope.
    pub root: usize,
}

impl ScopeTree {
    /// Create a new empty scope tree with a single global root scope.
    pub fn new() -> Self {
        let scopes = vec![Scope {
            kind: ScopeKind::Global,
            parent: None,
            children: Vec::new(),
            symbols: HashMap::new(),
            name: None,
        }];
        ScopeTree { scopes, root: 0 }
    }

    /// Create a new scope as a child of `parent`.
    ///
    /// Returns the index of the newly created scope.
    pub fn add_scope(
        &mut self,
        parent: usize,
        kind: ScopeKind,
        name: Option<String>,
    ) -> usize {
        let idx = self.scopes.len();
        self.scopes.push(Scope {
            kind,
            parent: Some(parent),
            children: Vec::new(),
            symbols: HashMap::new(),
            name,
        });
        self.scopes[parent].children.push(idx);
        idx
    }

    /// Declare a symbol in the given scope.
    ///
    /// Returns `Err` with a message if a symbol with the same name already
    /// exists in this scope.
    pub fn declare(
        &mut self,
        scope_idx: usize,
        symbol: Symbol,
    ) -> Result<(), String> {
        let name = symbol.name.clone();
        if self.scopes[scope_idx].symbols.contains_key(&name) {
            return Err(format!(
                "Duplicate declaration of '{}' in scope {:?}",
                name,
                self.scopes[scope_idx].kind
            ));
        }
        self.scopes[scope_idx].symbols.insert(name, symbol);
        Ok(())
    }

    /// Look up a symbol by name, searching the current scope and all ancestor
    /// scopes.
    pub fn lookup(&self, scope_idx: usize, name: &str) -> Option<&Symbol> {
        let mut idx = scope_idx;
        loop {
            if let Some(symbol) = self.scopes[idx].symbols.get(name) {
                return Some(symbol);
            }
            match self.scopes[idx].parent {
                Some(parent) => idx = parent,
                None => return None,
            }
        }
    }

    /// Look up a symbol by name only in the current scope, without searching
    /// ancestors.
    pub fn lookup_local(&self, scope_idx: usize, name: &str) -> Option<&Symbol> {
        self.scopes[scope_idx].symbols.get(name)
    }

    /// Collect all symbols declared in the given scope and all its descendant
    /// scopes.
    pub fn symbols_in_subtree(&self, scope_idx: usize) -> Vec<&Symbol> {
        let mut result = Vec::new();
        self.collect_symbols(scope_idx, &mut result);
        result
    }

    fn collect_symbols<'a>(
        &'a self,
        scope_idx: usize,
        result: &mut Vec<&'a Symbol>,
    ) {
        for symbol in self.scopes[scope_idx].symbols.values() {
            result.push(symbol);
        }
        let children = self.scopes[scope_idx].children.clone();
        for child in children {
            self.collect_symbols(child, result);
        }
    }
}

impl Default for ScopeTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a [`ScopeTree`] by walking a [`Program`] AST using the [`Visitor`]
/// trait.
pub struct ScopeTreeBuilder {
    /// The scope tree being built.
    pub tree: ScopeTree,
    /// Index of the scope we are currently visiting.
    current_scope: usize,
}

impl ScopeTreeBuilder {
    /// Create a new builder with an empty scope tree.
    pub fn new() -> Self {
        let tree = ScopeTree::new();
        let current_scope = tree.root;
        ScopeTreeBuilder {
            tree,
            current_scope,
        }
    }

    /// Walk the given `program` and produce the final [`ScopeTree`].
    pub fn build(mut self, program: &Program) -> ScopeTree {
        self.visit_program(program);
        self.tree
    }
}

impl Default for ScopeTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor for ScopeTreeBuilder {
    fn visit_program(&mut self, program: &Program) {
        for item in &program.items {
            self.visit_top_level_item(item);
        }
    }

    fn visit_top_level_item(&mut self, item: &TopLevelItem) {
        match item {
            TopLevelItem::Function(f) => {
                let func_scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Function,
                    Some(f.name.clone()),
                );
                let parent = self.current_scope;
                self.current_scope = func_scope;

                for param in &f.parameters {
                    let _ = self.tree.declare(
                        self.current_scope,
                        Symbol {
                            name: param.name.clone(),
                            kind: SymbolKind::Parameter,
                            declared_at: param.span,
                            type_name: param.type_.as_ref().map(|t| t.name.clone()),
                        },
                    );
                }

                for stmt in &f.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            TopLevelItem::Command(cmd) => {
                let cmd_scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Command,
                    Some(cmd.name.clone()),
                );
                let parent = self.current_scope;
                self.current_scope = cmd_scope;

                for arg in &cmd.arguments {
                    let _ = self.tree.declare(
                        self.current_scope,
                        Symbol {
                            name: arg.name.clone(),
                            kind: SymbolKind::Parameter,
                            declared_at: arg.span,
                            type_name: arg.type_.as_ref().map(|t| t.name.clone()),
                        },
                    );
                }

                for stmt in &cmd.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            TopLevelItem::Event(evt) => {
                let evt_scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Event,
                    Some(evt.event_type.clone()),
                );
                let parent = self.current_scope;
                self.current_scope = evt_scope;

                for stmt in &evt.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            TopLevelItem::PeriodicEvent(evt) => {
                let evt_scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Event,
                    Some(evt.period.clone()),
                );
                let parent = self.current_scope;
                self.current_scope = evt_scope;

                for stmt in &evt.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            _ => walk_top_level_item(self, item),
        }
    }

    fn visit_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::If(s) => {
                let if_scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Block,
                    None,
                );
                let parent = self.current_scope;
                self.current_scope = if_scope;

                for stmt in &s.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;

                for elif in &s.else_if {
                    let elif_scope = self.tree.add_scope(
                        self.current_scope,
                        ScopeKind::Block,
                        None,
                    );
                    self.current_scope = elif_scope;

                    for stmt in &elif.body {
                        self.visit_statement(stmt);
                    }

                    self.current_scope = parent;
                }

                if let Some(ref el) = s.else_branch {
                    let else_scope = self.tree.add_scope(
                        self.current_scope,
                        ScopeKind::Block,
                        None,
                    );
                    self.current_scope = else_scope;

                    for stmt in &el.body {
                        self.visit_statement(stmt);
                    }

                    self.current_scope = parent;
                }
            }
            Statement::While(s) => {
                let scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Block,
                    None,
                );
                let parent = self.current_scope;
                self.current_scope = scope;

                for stmt in &s.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            Statement::DoWhile(s) => {
                let scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Block,
                    None,
                );
                let parent = self.current_scope;
                self.current_scope = scope;

                for stmt in &s.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            Statement::ForIn(s) => {
                let scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Block,
                    Some(s.variable.clone()),
                );
                let parent = self.current_scope;
                self.current_scope = scope;

                let _ = self.tree.declare(
                    self.current_scope,
                    Symbol {
                        name: s.variable.clone(),
                        kind: SymbolKind::LoopVariable,
                        declared_at: s.span,
                        type_name: None,
                    },
                );

                for stmt in &s.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            Statement::Loop(s) => {
                let scope = self.tree.add_scope(
                    self.current_scope,
                    ScopeKind::Block,
                    None,
                );
                let parent = self.current_scope;
                self.current_scope = scope;

                for stmt in &s.body {
                    self.visit_statement(stmt);
                }

                self.current_scope = parent;
            }
            Statement::Assignment(s) => {
                if let Some(var_name) = get_assignment_target_name(&s.target) {
                    if !var_name.starts_with('{') {
                        let _ = self.tree.declare(
                            self.current_scope,
                            Symbol {
                                name: var_name.clone(),
                                kind: SymbolKind::Variable,
                                declared_at: s.span,
                                type_name: None,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract a variable name from an assignment target, if it is a simple
/// variable assignment.
fn get_assignment_target_name(target: &AssignmentTarget) -> Option<String> {
    match target {
        AssignmentTarget::Variable(v) => Some(v.name.clone()),
        AssignmentTarget::PropertyAccess(_) | AssignmentTarget::Index(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_program;

    #[test]
    fn test_empty_program() {
        let result = parse_program("");
        let tree = ScopeTreeBuilder::new().build(&result.program);
        assert_eq!(tree.scopes.len(), 1);
        assert_eq!(tree.scopes[tree.root].kind, ScopeKind::Global);
        assert!(tree.scopes[tree.root].symbols.is_empty());
    }

    #[test]
    fn test_function_scope() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        assert_eq!(tree.scopes.len(), 2);
        let func_scope = &tree.scopes[1];
        assert_eq!(func_scope.kind, ScopeKind::Function);
        assert_eq!(func_scope.name.as_deref(), Some("greet"));
        assert_eq!(func_scope.parent, Some(0));
        assert!(tree.scopes[0].children.contains(&1));
    }

    #[test]
    fn test_parameter_symbols() {
        let source = "function greet(name: text, age: number):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        let func_scope = &tree.scopes[1];
        let name_sym = func_scope.symbols.get("name").expect("missing 'name'");
        assert_eq!(name_sym.kind, SymbolKind::Parameter);
        assert_eq!(name_sym.type_name.as_deref(), Some("text"));
        let age_sym = func_scope.symbols.get("age").expect("missing 'age'");
        assert_eq!(age_sym.kind, SymbolKind::Parameter);
        assert_eq!(age_sym.type_name.as_deref(), Some("number"));
    }

    #[test]
    fn test_if_block_scope() {
        let source = "function test():\n\tif true:\n\t\tsend \"yes\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Global(0) > Function(1) > Block(2)
        assert_eq!(tree.scopes.len(), 3);
        assert_eq!(tree.scopes[2].kind, ScopeKind::Block);
        assert_eq!(tree.scopes[2].parent, Some(1));
        assert!(tree.scopes[1].children.contains(&2));
    }

    #[test]
    fn test_nested_scopes() {
        let source =
            "function test():\n\twhile true:\n\t\tif true:\n\t\t\tsend \"yes\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Global(0) > Function(1) > Block-while(2) > Block-if(3)
        assert_eq!(tree.scopes.len(), 4);
        assert_eq!(tree.scopes[1].kind, ScopeKind::Function);
        assert_eq!(tree.scopes[2].kind, ScopeKind::Block);
        assert_eq!(tree.scopes[3].kind, ScopeKind::Block);
        assert_eq!(tree.scopes[2].parent, Some(1));
        assert_eq!(tree.scopes[3].parent, Some(2));
    }

    #[test]
    fn test_lookup_ancestor() {
        let source =
            "function test(name: text):\n\tif true:\n\t\tset x to name";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // block scope = index 2 (if body under function)
        let symbol = tree.lookup(2, "name");
        assert!(symbol.is_some(), "should find 'name' via ancestor lookup");
        assert_eq!(symbol.unwrap().kind, SymbolKind::Parameter);
    }

    #[test]
    fn test_lookup_local_only() {
        let source =
            "function test():\n\tset y to 10\n\tif true:\n\t\tset x to 5";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // function scope = 1, if block scope = 2
        let func_idx = 1;
        let block_idx = 2;
        // 'x' is in block scope
        assert!(tree.lookup_local(block_idx, "x").is_some());
        // 'y' is in function scope, NOT in block scope
        assert!(tree.lookup_local(block_idx, "y").is_none());
        // but can be found via ancestor lookup
        assert!(tree.lookup(block_idx, "y").is_some());
        // 'x' is visible from function scope? No — it's in the child block
        assert!(tree.lookup_local(func_idx, "x").is_none());
    }

    #[test]
    fn test_duplicate_declaration() {
        let mut tree = ScopeTree::new();
        let root = tree.root;
        assert!(tree
            .declare(
                root,
                Symbol {
                    name: "x".into(),
                    kind: SymbolKind::Variable,
                    declared_at: Span::new(0, 1),
                    type_name: None,
                }
            )
            .is_ok());
        let result = tree.declare(
            root,
            Symbol {
                name: "x".into(),
                kind: SymbolKind::Variable,
                declared_at: Span::new(2, 3),
                type_name: None,
            },
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn test_for_in_loop_variable() {
        let source =
            "function test():\n\tfor each element in {_list}:\n\t\tsend element";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Global(0) > Function(1) > Block(2)
        assert!(tree.scopes.len() >= 3);
        let loop_scope = &tree.scopes[2];
        let el_sym = loop_scope
            .symbols
            .get("element")
            .expect("loop variable 'element' should be registered");
        assert_eq!(el_sym.kind, SymbolKind::LoopVariable);
    }

    #[test]
    fn test_multiple_functions() {
        let source =
            "function a():\n\treturn\nfunction b():\n\treturn";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Global(0) + Function-a(1) + Function-b(2)
        assert_eq!(tree.scopes.len(), 3);
        assert_eq!(tree.scopes[1].kind, ScopeKind::Function);
        assert_eq!(tree.scopes[1].name.as_deref(), Some("a"));
        assert_eq!(tree.scopes[2].kind, ScopeKind::Function);
        assert_eq!(tree.scopes[2].name.as_deref(), Some("b"));
        assert!(tree.scopes[0].children.contains(&1));
        assert!(tree.scopes[0].children.contains(&2));
    }

    #[test]
    fn test_command_scope() {
        let source = "command /hello <text>:\n\ttrigger:\n\t\tsend \"hi\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Global(0) + Command(1)
        assert_eq!(tree.scopes.len(), 2);
        let cmd_scope = &tree.scopes[1];
        assert_eq!(cmd_scope.kind, ScopeKind::Command);
        assert_eq!(cmd_scope.name.as_deref(), Some("/hello"));
        let arg_sym = cmd_scope
            .symbols
            .get("text")
            .expect("command arg 'text' should be registered");
        assert_eq!(arg_sym.kind, SymbolKind::Parameter);
    }

    #[test]
    fn test_event_scope() {
        let source = "on first join:\n\tbroadcast \"welcome\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        assert_eq!(tree.scopes.len(), 2);
        let evt_scope = &tree.scopes[1];
        assert_eq!(evt_scope.kind, ScopeKind::Event);
        assert_eq!(evt_scope.name.as_deref(), Some("first join"));
    }

    #[test]
    fn test_else_if_else_scopes() {
        let source = "function test():\n\tif true:\n\t\tsend \"a\"\n\telse if false:\n\t\tsend \"b\"\n\telse:\n\t\tsend \"c\"";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Global(0) > Function(1) > if-block(2), elif-block(3), else-block(4)
        assert_eq!(tree.scopes.len(), 5);
        assert_eq!(tree.scopes[2].kind, ScopeKind::Block);
        assert_eq!(tree.scopes[3].kind, ScopeKind::Block);
        assert_eq!(tree.scopes[4].kind, ScopeKind::Block);
        // All three blocks are children of the function scope
        assert!(tree.scopes[1].children.contains(&2));
        assert!(tree.scopes[1].children.contains(&3));
        assert!(tree.scopes[1].children.contains(&4));
    }

    #[test]
    fn test_symbols_in_subtree() {
        let source =
            "function test(name: text):\n\tif true:\n\t\tset x to 5";
        let result = parse_program(source);
        let tree = ScopeTreeBuilder::new().build(&result.program);
        // Collect all symbols from function scope (1) downward
        let all = tree.symbols_in_subtree(1);
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"x"));
    }
}
