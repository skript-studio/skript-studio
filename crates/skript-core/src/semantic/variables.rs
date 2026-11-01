use crate::ast::{
    walk_expression, walk_top_level_item, AssignmentTarget, Expression, Program, Span, Statement,
    TopLevelItem, Visitor,
};

/// Determine whether a variable name refers to a local variable.
/// Local variables in Skript start with `_` (e.g. `{_x}`).
pub fn is_local(name: &str) -> bool {
    name.starts_with('_')
}

/// Check if a name is a built-in loop variable.
pub fn is_loop_variable(name: &str) -> bool {
    matches!(
        name,
        "loop-value"
            | "loop-player"
            | "loop-block"
            | "loop-item"
            | "loop-index"
            | "loop-number"
            | "loop-entity"
    )
}

/// Remove surrounding braces from a variable name if present.
/// Note: the Skript parser already strips braces, so this is
/// effectively a no-op for parsed AST names but provided for
/// API completeness.
pub fn strip_braces(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extract the base name of a list variable by removing `::*` suffix.
/// `{list::*}` → `"list"`, `{_list::*}` → `"_list"`.
pub fn extract_list_base(name: &str) -> String {
    if let Some(base) = name.strip_suffix("::*") {
        base.to_string()
    } else {
        name.to_string()
    }
}

/// The kind of a tracked variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    /// Local variable (name starts with `_`).
    Local,
    /// Global variable (name does not start with `_`).
    Global,
    /// Loop variable (for-in loop variable).
    LoopVar,
    /// Function or command parameter.
    Parameter,
}

/// A recorded variable declaration.
#[derive(Debug, Clone)]
pub struct VariableDecl {
    pub name: String,
    pub kind: VarKind,
    pub declared_at: Span,
    pub scope_idx: usize,
    pub usages: Vec<usize>,
}

/// A recorded variable usage (read-site reference).
#[derive(Debug, Clone)]
pub struct VariableUsage {
    pub name: String,
    pub used_at: Span,
    pub scope_idx: usize,
}

/// A diagnostic error or warning related to variable analysis.
#[derive(Debug, Clone)]
pub struct VariableError {
    pub message: String,
    pub span: Span,
}

/// Tracks variable declarations and usages across a Skript program.
///
/// Built by [`VariableTrackerBuilder`].
#[derive(Debug, Clone)]
pub struct VariableTracker {
    pub declarations: Vec<VariableDecl>,
    pub usages: Vec<VariableUsage>,
    pub errors: Vec<VariableError>,
}

impl VariableTracker {
    pub fn new() -> Self {
        VariableTracker {
            declarations: Vec::new(),
            usages: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Return all local variable declarations that have zero usages.
    pub fn unused_locals(&self) -> Vec<&VariableDecl> {
        self.declarations
            .iter()
            .filter(|d| d.kind == VarKind::Local && d.usages.is_empty())
            .collect()
    }
}

impl Default for VariableTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine [`VarKind`] from a bare variable name.
fn kind_from_name(name: &str) -> VarKind {
    if name.starts_with('_') {
        VarKind::Local
    } else {
        VarKind::Global
    }
}

/// Builds a [`VariableTracker`] by walking a [`Program`] AST.
///
/// The builder maintains its own lightweight scope hierarchy (separate from
/// [`ScopeTree`]) so it can associate declarations and usages with their
/// enclosing scope.
pub struct VariableTrackerBuilder {
    pub tracker: VariableTracker,
    /// Stack of scope IDs — current scope is at the top.
    scope_stack: Vec<usize>,
    /// Parent of each scope ID (None for root).
    scope_parents: Vec<Option<usize>>,
    next_scope_id: usize,
}

impl VariableTrackerBuilder {
    pub fn new() -> Self {
        VariableTrackerBuilder {
            tracker: VariableTracker::new(),
            scope_stack: vec![0],
            scope_parents: vec![None],
            next_scope_id: 1,
        }
    }

    /// Walk the given `program` and produce the final [`VariableTracker`].
    pub fn build(mut self, program: &Program) -> VariableTracker {
        self.visit_program(program);
        self.detect_errors();
        self.tracker
    }

    fn current_scope(&self) -> usize {
        *self.scope_stack.last().unwrap()
    }

    fn push_scope(&mut self) -> usize {
        let parent = self.current_scope();
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        self.scope_parents.push(Some(parent));
        self.scope_stack.push(id);
        id
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn is_ancestor_or_equal(&self, ancestor: usize, descendant: usize) -> bool {
        let mut id = descendant;
        loop {
            if id == ancestor {
                return true;
            }
            match self.scope_parents[id] {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }

    fn record_declaration(&mut self, name: &str, span: Span) {
        let kind = kind_from_name(name);
        self.tracker.declarations.push(VariableDecl {
            name: name.to_string(),
            kind,
            declared_at: span,
            scope_idx: self.current_scope(),
            usages: Vec::new(),
        });
    }

    fn record_param(&mut self, name: &str, span: Span) {
        self.tracker.declarations.push(VariableDecl {
            name: name.to_string(),
            kind: VarKind::Parameter,
            declared_at: span,
            scope_idx: self.current_scope(),
            usages: Vec::new(),
        });
    }

    fn record_loop_var(&mut self, name: &str, span: Span) {
        self.tracker.declarations.push(VariableDecl {
            name: name.to_string(),
            kind: VarKind::LoopVar,
            declared_at: span,
            scope_idx: self.current_scope(),
            usages: Vec::new(),
        });
    }

    fn record_usage(&mut self, name: &str, span: Span) {
        let idx = self.tracker.usages.len();
        let current = self.current_scope();
        self.tracker.usages.push(VariableUsage {
            name: name.to_string(),
            used_at: span,
            scope_idx: current,
        });
        // Find the latest matching declaration visible from this scope
        // using indices to avoid mutable+immutable borrow conflict.
        let decl_idx = (0..self.tracker.declarations.len()).rev().find(|&i| {
            let d = &self.tracker.declarations[i];
            d.name == name && self.is_ancestor_or_equal(d.scope_idx, current)
        });
        if let Some(di) = decl_idx {
            self.tracker.declarations[di].usages.push(idx);
        }
    }

    fn detect_errors(&mut self) {
        for decl in &self.tracker.declarations {
            // Unused local variable warning
            if decl.kind == VarKind::Local && decl.usages.is_empty() {
                self.tracker.errors.push(VariableError {
                    message: format!("Unused local variable '{}'", decl.name),
                    span: decl.declared_at,
                });
            }
            // Local variable referenced outside its declaring scope
            if decl.kind == VarKind::Local {
                for &usage_idx in &decl.usages {
                    let usage = &self.tracker.usages[usage_idx];
                    if !self.is_ancestor_or_equal(decl.scope_idx, usage.scope_idx) {
                        self.tracker.errors.push(VariableError {
                            message: format!(
                                "Local variable '{}' used outside its declaring scope",
                                decl.name
                            ),
                            span: usage.used_at,
                        });
                    }
                }
            }
        }
    }
}

impl Default for VariableTrackerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor for VariableTrackerBuilder {
    fn visit_program(&mut self, program: &Program) {
        for item in &program.items {
            self.visit_top_level_item(item);
        }
    }

    fn visit_top_level_item(&mut self, item: &TopLevelItem) {
        match item {
            TopLevelItem::Function(f) => {
                self.push_scope();
                for param in &f.parameters {
                    self.record_param(&param.name, param.span);
                }
                for stmt in &f.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            TopLevelItem::Command(cmd) => {
                self.push_scope();
                for arg in &cmd.arguments {
                    self.record_param(&arg.name, arg.span);
                }
                for stmt in &cmd.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            TopLevelItem::Event(evt) => {
                self.push_scope();
                for stmt in &evt.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            TopLevelItem::PeriodicEvent(evt) => {
                self.push_scope();
                for stmt in &evt.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            TopLevelItem::Example(example) => {
                self.push_scope();
                for stmt in &example.content {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            _ => walk_top_level_item(self, item),
        }
    }

    fn visit_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::If(s) => {
                self.visit_expression(&s.condition);

                self.push_scope();
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();

                for elif in &s.else_if {
                    self.visit_expression(&elif.condition);
                    self.push_scope();
                    for stmt in &elif.body {
                        self.visit_statement(stmt);
                    }
                    self.pop_scope();
                }

                if let Some(ref el) = s.else_branch {
                    self.push_scope();
                    for stmt in &el.body {
                        self.visit_statement(stmt);
                    }
                    self.pop_scope();
                }
            }
            Statement::While(s) => {
                self.visit_expression(&s.condition);
                self.push_scope();
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::DoWhile(s) => {
                self.push_scope();
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
                self.visit_expression(&s.condition);
            }
            Statement::ForIn(s) => {
                self.visit_expression(&s.iterable);

                self.push_scope();
                self.record_loop_var(&s.variable, s.span);
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Loop(s) => {
                self.push_scope();
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Assignment(s) => {
                if let AssignmentTarget::Variable(v) = &s.target {
                    self.record_declaration(&v.name, v.span);
                }
                self.visit_expression(&s.value);
            }
            Statement::Effect(s) => {
                for arg in &s.arguments {
                    self.visit_expression(arg);
                }
            }
            Statement::Return(s) => {
                if let Some(ref val) = s.value {
                    self.visit_expression(val);
                }
            }
            Statement::Expression(expr) => {
                self.visit_expression(expr);
            }
            Statement::Wait(s) => {
                if let Some(ref dur) = s.duration {
                    self.visit_expression(dur);
                }
            }
            _ => {}
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Variable(v) => {
                self.record_usage(&v.name, v.span);
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
    fn test_local_used() {
        let source = "function test():\n\tset {_x} to 5\n\tbroadcast {_x}";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        assert!(
            tracker.errors.is_empty(),
            "expected no errors, got: {:?}",
            tracker.errors
        );
        assert_eq!(tracker.declarations.len(), 1);
        assert_eq!(tracker.usages.len(), 1);
        assert_eq!(tracker.declarations[0].usages.len(), 1);
    }

    #[test]
    fn test_local_unused() {
        let source = "function test():\n\tset {_x} to 5";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        assert_eq!(tracker.errors.len(), 1);
        assert!(tracker.errors[0].message.contains("Unused local"));
        assert!(tracker.errors[0].message.contains("_x"));
    }

    #[test]
    fn test_global_no_warning() {
        let source = "function test():\n\tset {x} to 5";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        assert!(
            tracker.errors.is_empty(),
            "expected no errors for unused global, got: {:?}",
            tracker.errors
        );
    }

    #[test]
    fn test_loop_variable_tracked() {
        let source = "function test():\n\tfor each element in {_list}:\n\t\tbroadcast element";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        assert!(
            tracker.errors.is_empty(),
            "expected no errors, got: {:?}",
            tracker.errors
        );
        // loop variable 'element' declaration + usage inside loop
        let loop_decl = tracker
            .declarations
            .iter()
            .find(|d| d.name == "element")
            .expect("loop variable should be declared");
        assert_eq!(loop_decl.kind, VarKind::LoopVar);
        assert!(
            !loop_decl.usages.is_empty(),
            "loop variable should have usages"
        );
    }

    #[test]
    fn test_parameter_tracked() {
        let source = "function test(name: text):\n\tbroadcast {_name}";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        let param_decl = tracker
            .declarations
            .iter()
            .find(|d| d.name == "name")
            .expect("parameter should be declared");
        assert_eq!(param_decl.kind, VarKind::Parameter);
    }

    #[test]
    fn test_multiple_scopes() {
        let source =
            "function test():\n\tset {_a} to 1\n\tif true:\n\t\tset {_b} to 2\n\tbroadcast {_a}";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        // _a declared in function scope, used → no error
        // _b declared in if-block scope, unused → warning
        let a_decl = tracker
            .declarations
            .iter()
            .find(|d| d.name == "_a")
            .expect("_a should be declared");
        assert_eq!(a_decl.usages.len(), 1);
        let b_decl = tracker
            .declarations
            .iter()
            .find(|d| d.name == "_b")
            .expect("_b should be declared");
        assert!(b_decl.usages.is_empty());
        let unused: Vec<_> = tracker
            .errors
            .iter()
            .filter(|e| e.message.contains("Unused"))
            .collect();
        assert_eq!(unused.len(), 1);
        assert!(unused[0].message.contains("_b"));
    }

    #[test]
    fn test_is_local() {
        assert!(is_local("_x"));
        assert!(is_local("_list::*"));
        assert!(!is_local("x"));
        assert!(!is_local("list::*"));
    }

    #[test]
    fn test_is_loop_variable() {
        assert!(is_loop_variable("loop-value"));
        assert!(is_loop_variable("loop-index"));
        assert!(!is_loop_variable("_x"));
    }

    #[test]
    fn test_strip_braces() {
        assert_eq!(strip_braces("{_x}"), "_x");
        assert_eq!(strip_braces("{list::*}"), "list::*");
        assert_eq!(strip_braces("_x"), "_x");
    }

    #[test]
    fn test_extract_list_base() {
        assert_eq!(extract_list_base("list::*"), "list");
        assert_eq!(extract_list_base("_list::*"), "_list");
        assert_eq!(extract_list_base("_x"), "_x");
    }

    #[test]
    fn test_empty_program() {
        let result = parse_program("");
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        assert!(tracker.declarations.is_empty());
        assert!(tracker.usages.is_empty());
        assert!(tracker.errors.is_empty());
    }

    #[test]
    fn test_local_used_via_effect() {
        let source = "function test():\n\tset {_x} to \"hello\"\n\tbroadcast {_x}";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        assert!(
            tracker.errors.is_empty(),
            "expected no errors, got: {:?}",
            tracker.errors
        );
        assert_eq!(tracker.declarations[0].usages.len(), 1);
    }

    #[test]
    fn test_local_reassigned() {
        let source = "function test():\n\tset {_x} to 1\n\tset {_x} to 2\n\tbroadcast {_x}";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        // Two declarations of _x: the first is immediately overwritten
        // (unused), the second gets the usage from broadcast.
        let unused: Vec<_> = tracker
            .errors
            .iter()
            .filter(|e| e.message.contains("Unused"))
            .collect();
        assert_eq!(unused.len(), 1);
        assert!(unused[0].message.contains("_x"));
    }

    #[test]
    fn test_unused_locals_api() {
        let source = "function test():\n\tset {_x} to 1\n\tset {_y} to 2\n\tbroadcast {_x}";
        let result = parse_program(source);
        let tracker = VariableTrackerBuilder::new().build(&result.program);
        let unused = tracker.unused_locals();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "_y");
    }
}
