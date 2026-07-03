use std::collections::HashMap;

use crate::ast::*;
use crate::semantic::functions::FunctionRegistry;
use crate::semantic::scope::ScopeTree;
use crate::semantic::scope::Symbol;
use crate::semantic::scope::SymbolKind;

/// A resolved type for an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeInfo {
    Known(String),
    Unknown,
    Any,
    Void,
}

/// Maps AST nodes to their inferred types.
pub struct TypeInference {
    pub expr_types: HashMap<usize, TypeInfo>,
    pub errors: Vec<String>,
}

impl TypeInference {
    pub fn new() -> Self {
        TypeInference {
            expr_types: HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn get_type(&self, span_start: usize) -> &TypeInfo {
        self.expr_types.get(&span_start).unwrap_or(&TypeInfo::Unknown)
    }
}

impl Default for TypeInference {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TypeInferenceBuilder {
    inference: TypeInference,
    scopes: ScopeTree,
    functions: FunctionRegistry,
    current_scope: usize,
}

impl TypeInferenceBuilder {
    pub fn new(scopes: ScopeTree, functions: FunctionRegistry) -> Self {
        let current_scope = scopes.root;
        TypeInferenceBuilder {
            inference: TypeInference::new(),
            scopes,
            functions,
            current_scope,
        }
    }

    pub fn build(mut self, program: &Program) -> TypeInference {
        self.visit_program(program);
        self.inference
    }

    fn infer_expression(&mut self, expr: &Expression) -> TypeInfo {
        let ty = match expr {
            Expression::Literal(lit) => self.infer_literal(lit),
            Expression::Variable(v) => self.infer_variable(v),
            Expression::FunctionCall(call) => self.infer_function_call(call),
            Expression::Binary(bin) => self.infer_binary(bin),
            Expression::Unary(u) => self.infer_unary(u),
            Expression::Ternary(t) => self.infer_ternary(t),
            Expression::PropertyAccess(prop) => self.infer_property_access(prop),
            Expression::Index(_) => TypeInfo::Any,
            Expression::EventValue(_) => TypeInfo::Any,
            Expression::List(_) => TypeInfo::Any,
            Expression::Parenthesized(p) => self.infer_expression(&p.expression),
            Expression::StringInterpolation(_) => TypeInfo::Known("text".to_string()),
            Expression::Unknown(_) => TypeInfo::Unknown,
        };

        let key = expr_span_start(expr);
        self.inference.expr_types.insert(key, ty.clone());
        ty
    }

    fn infer_literal(&self, lit: &Literal) -> TypeInfo {
        match lit {
            Literal::Number(_) => TypeInfo::Known("number".to_string()),
            Literal::String(_) => TypeInfo::Known("text".to_string()),
            Literal::Boolean(_) => TypeInfo::Known("boolean".to_string()),
        }
    }

    fn infer_variable(&self, v: &Variable) -> TypeInfo {
        if let Some(sym) = self.scopes.lookup(self.current_scope, &v.name) {
            if let Some(ref tn) = sym.type_name {
                return TypeInfo::Known(tn.clone());
            }
        }
        TypeInfo::Any
    }

    fn infer_function_call(&self, call: &FunctionCallExpr) -> TypeInfo {
        let func_name = match call.callee.as_ref() {
            Expression::Variable(v) => &v.name,
            _ => return TypeInfo::Unknown,
        };
        if let Some(info) = self.functions.get(func_name) {
            if let Some(ref rt) = info.decl.return_type {
                return TypeInfo::Known(rt.name.clone());
            }
        }
        TypeInfo::Unknown
    }

    fn infer_binary(&mut self, bin: &BinaryExpr) -> TypeInfo {
        let left_ty = self.infer_expression(&bin.left);
        let right_ty = self.infer_expression(&bin.right);

        match bin.operator {
            BinaryOp::Concat => TypeInfo::Known("text".to_string()),
            BinaryOp::And | BinaryOp::Or => TypeInfo::Known("boolean".to_string()),
            BinaryOp::Eq | BinaryOp::Neq | BinaryOp::Lt | BinaryOp::Gt
            | BinaryOp::Leq | BinaryOp::Geq | BinaryOp::In | BinaryOp::InstanceOf => {
                TypeInfo::Known("boolean".to_string())
            }
            BinaryOp::Add => {
                if left_ty == TypeInfo::Known("text".to_string())
                    || right_ty == TypeInfo::Known("text".to_string())
                {
                    TypeInfo::Known("text".to_string())
                } else {
                    TypeInfo::Known("number".to_string())
                }
            }
            _ => TypeInfo::Known("number".to_string()),
        }
    }

    fn infer_unary(&mut self, u: &UnaryExpr) -> TypeInfo {
        let _operand_ty = self.infer_expression(&u.operand);
        match u.operator {
            UnaryOp::Not => TypeInfo::Known("boolean".to_string()),
            UnaryOp::Neg => TypeInfo::Known("number".to_string()),
            UnaryOp::Length => TypeInfo::Known("number".to_string()),
        }
    }

    fn infer_ternary(&mut self, t: &TernaryExpr) -> TypeInfo {
        let then_ty = self.infer_expression(&t.then_branch);
        let else_ty = self.infer_expression(&t.else_branch);
        if then_ty == else_ty {
            then_ty
        } else {
            TypeInfo::Any
        }
    }

    fn infer_property_access(&self, prop: &PropertyAccessExpr) -> TypeInfo {
        let known_properties: HashMap<&str, &str> = [
            ("player", "player"),
            ("world", "world"),
            ("location", "location"),
            ("block", "block"),
            ("inventory", "inventory"),
            ("health", "number"),
            ("food", "number"),
            ("level", "number"),
            ("exp", "number"),
            ("name", "text"),
            ("display name", "text"),
            ("uuid", "text"),
            ("x", "number"),
            ("y", "number"),
            ("z", "number"),
            ("yaw", "number"),
            ("pitch", "number"),
            ("size", "number"),
            ("length", "number"),
            ("height", "number"),
            ("width", "number"),
            ("max health", "number"),
            ("walk speed", "number"),
            ("fly speed", "number"),
            ("item", "item"),
            ("tool", "item"),
            ("weapon", "item"),
            ("armor", "item"),
            ("type", "text"),
        ]
        .iter()
        .cloned()
        .collect();

        if let Some(&prop_type) = known_properties.get(prop.property.as_str()) {
            TypeInfo::Known(prop_type.to_string())
        } else {
            TypeInfo::Any
        }
    }
}

impl Visitor for TypeInferenceBuilder {
    fn visit_program(&mut self, program: &Program) {
        for item in &program.items {
            self.visit_top_level_item(item);
        }
    }

    fn visit_top_level_item(&mut self, item: &TopLevelItem) {
        match item {
            TopLevelItem::Function(f) => {
                let parent = self.current_scope;
                if let Some(child) = self.scopes.scopes[parent]
                    .children
                    .iter()
                    .find(|&&idx| self.scopes.scopes[idx].name == Some(f.name.clone()))
                {
                    self.current_scope = *child;
                }
                for stmt in &f.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
            }
            TopLevelItem::Command(cmd) => {
                let parent = self.current_scope;
                if let Some(child) = self.scopes.scopes[parent]
                    .children
                    .iter()
                    .find(|&&idx| self.scopes.scopes[idx].name == Some(cmd.name.clone()))
                {
                    self.current_scope = *child;
                }
                for stmt in &cmd.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
            }
            TopLevelItem::Event(evt) => {
                let parent = self.current_scope;
                if let Some(child) = self.scopes.scopes[parent]
                    .children
                    .iter()
                    .find(|&&idx| self.scopes.scopes[idx].name == Some(evt.event_type.clone()))
                {
                    self.current_scope = *child;
                }
                for stmt in &evt.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
            }
            TopLevelItem::PeriodicEvent(evt) => {
                let parent = self.current_scope;
                if let Some(child) = self.scopes.scopes[parent]
                    .children
                    .iter()
                    .find(|&&idx| self.scopes.scopes[idx].name == Some(evt.period.clone()))
                {
                    self.current_scope = *child;
                }
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
                self.infer_expression(&s.condition);
                let parent = self.current_scope;
                if let Some(child) = self.scopes.scopes[parent]
                    .children
                    .iter()
                    .find(|&&idx| {
                        self.scopes.scopes[idx].kind == crate::semantic::scope::ScopeKind::Block
                            && !self.scopes.scopes[idx].children.is_empty()
                    }) {
                    self.current_scope = *child;
                }
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;

                for elif in &s.else_if {
                    self.infer_expression(&elif.condition);
                }
                if let Some(ref el) = s.else_branch {
                    let children: Vec<usize> = self.scopes.scopes[parent].children.clone();
                    let mut found = false;
                    for &idx in &children {
                        if idx != self.current_scope {
                            let saved = self.current_scope;
                            self.current_scope = idx;
                            for stmt in &el.body {
                                self.visit_statement(stmt);
                            }
                            self.current_scope = saved;
                            found = true;
                        }
                    }
                    if !found {
                        for stmt in &el.body {
                            self.visit_statement(stmt);
                        }
                    }
                }
            }
            Statement::Loop(s) => {
                let parent = self.current_scope;
                if let Some(&child) = self.scopes.scopes[parent].children.first() {
                    if self.scopes.scopes[child].kind == crate::semantic::scope::ScopeKind::Block {
                        self.current_scope = child;
                    }
                }
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
            }
            Statement::While(s) => {
                self.infer_expression(&s.condition);
                let parent = self.current_scope;
                if let Some(&child) = self.scopes.scopes[parent].children.first() {
                    if self.scopes.scopes[child].kind == crate::semantic::scope::ScopeKind::Block {
                        self.current_scope = child;
                    }
                }
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
            }
            Statement::DoWhile(s) => {
                let parent = self.current_scope;
                if let Some(&child) = self.scopes.scopes[parent].children.first() {
                    if self.scopes.scopes[child].kind == crate::semantic::scope::ScopeKind::Block {
                        self.current_scope = child;
                    }
                }
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
                self.infer_expression(&s.condition);
            }
            Statement::ForIn(s) => {
                self.infer_expression(&s.iterable);
                let parent = self.current_scope;
                if let Some(&child) = self.scopes.scopes[parent].children.first() {
                    if self.scopes.scopes[child].kind == crate::semantic::scope::ScopeKind::Block {
                        self.current_scope = child;
                    }
                }
                for stmt in &s.body {
                    self.visit_statement(stmt);
                }
                self.current_scope = parent;
            }
            Statement::Assignment(s) => {
                let val_ty = self.infer_expression(&s.value);
                if let AssignmentTarget::Variable(v) = &s.target {
                    self.inference.expr_types.insert(v.span.start, val_ty.clone());
                }
            }
            Statement::Effect(s) => {
                for arg in &s.arguments {
                    self.infer_expression(arg);
                }
            }
            Statement::Return(r) => {
                if let Some(ref val) = r.value {
                    self.infer_expression(val);
                }
            }
            Statement::Expression(expr) => {
                self.infer_expression(expr);
            }
            Statement::Wait(w) => {
                if let Some(ref dur) = w.duration {
                    self.infer_expression(dur);
                }
            }
            _ => {}
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        self.infer_expression(expr);
    }
}

fn expr_span_start(expr: &Expression) -> usize {
    match expr {
        Expression::Binary(e) => e.span.start,
        Expression::Unary(e) => e.span.start,
        Expression::Ternary(e) => e.span.start,
        Expression::PropertyAccess(e) => e.span.start,
        Expression::FunctionCall(e) => e.span.start,
        Expression::Index(e) => e.span.start,
        Expression::Literal(lit) => match lit {
            Literal::Number(n) => n.span.start,
            Literal::String(s) => s.span.start,
            Literal::Boolean(b) => b.span.start,
        },
        Expression::Variable(v) => v.span.start,
        Expression::EventValue(e) => e.span.start,
        Expression::List(e) => e.span.start,
        Expression::Parenthesized(e) => e.span.start,
        Expression::StringInterpolation(e) => e.span.start,
        Expression::Unknown(e) => e.span.start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn num_literal(value: &str, start: usize) -> Expression {
        Expression::Literal(Literal::Number(NumberLiteral {
            value: value.into(),
            span: Span::new(start, start + value.len()),
        }))
    }

    fn str_literal(value: &str, start: usize) -> Expression {
        Expression::Literal(Literal::String(StringLiteral {
            value: value.into(),
            span: Span::new(start, start + value.len() + 2),
        }))
    }

    fn bool_literal(value: bool, start: usize) -> Expression {
        let s = if value { "true" } else { "false" };
        Expression::Literal(Literal::Boolean(BooleanLiteral {
            value,
            span: Span::new(start, start + s.len()),
        }))
    }

    fn variable(name: &str, start: usize) -> Expression {
        Expression::Variable(Variable {
            name: name.into(),
            span: Span::new(start, start + name.len()),
        })
    }

    fn function_call(name: &str, args: Vec<Expression>, start: usize, end: usize) -> Expression {
        Expression::FunctionCall(FunctionCallExpr {
            callee: Box::new(variable(name, start)),
            arguments: args,
            span: Span::new(start, end),
        })
    }

    #[test]
    fn test_number_literal() {
        let expr = num_literal("42", 0);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("number".to_string()));
    }

    #[test]
    fn test_string_literal() {
        let expr = str_literal("hello", 0);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("text".to_string()));
    }

    #[test]
    fn test_boolean_literal() {
        let expr = bool_literal(true, 0);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("boolean".to_string()));
    }

    #[test]
    fn test_variable_with_type_in_scope() {
        let mut tree = ScopeTree::new();
        tree.declare(
            tree.root,
            Symbol {
                name: "name".into(),
                kind: SymbolKind::Parameter,
                declared_at: Span::new(0, 4),
                type_name: Some("text".into()),
            },
        ).ok();
        let expr = variable("name", 5);
        let mut builder = TypeInferenceBuilder::new(tree, FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("text".to_string()));
    }

    #[test]
    fn test_variable_without_type() {
        let expr = variable("{_x}", 0);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Any);
    }

    #[test]
    fn test_function_call_return_type() {
        let mut registry = FunctionRegistry::new();
        registry
            .register(&FunctionDecl {
                name: "add".into(),
                parameters: vec![],
                return_type: Some(Type {
                    name: "number".into(),
                    span: Span::new(0, 6),
                }),
                body: vec![],
                span: Span::new(0, 0),
            })
            .ok();
        let expr = function_call("add", vec![num_literal("1", 4), num_literal("2", 6)], 0, 7);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), registry);
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("number".to_string()));
    }

    #[test]
    fn test_function_call_no_return_type() {
        let mut registry = FunctionRegistry::new();
        registry
            .register(&FunctionDecl {
                name: "foo".into(),
                parameters: vec![],
                return_type: None,
                body: vec![],
                span: Span::new(0, 0),
            })
            .ok();
        let expr = function_call("foo", vec![], 0, 5);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), registry);
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Unknown);
    }

    #[test]
    fn test_binary_add_number() {
        let expr = Expression::Binary(BinaryExpr {
            left: Box::new(num_literal("1", 0)),
            operator: BinaryOp::Add,
            right: Box::new(num_literal("2", 2)),
            span: Span::new(0, 3),
        });
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("number".to_string()));
    }

    #[test]
    fn test_binary_add_text() {
        let expr = Expression::Binary(BinaryExpr {
            left: Box::new(str_literal("hello", 0)),
            operator: BinaryOp::Add,
            right: Box::new(str_literal(" world", 7)),
            span: Span::new(0, 14),
        });
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("text".to_string()));
    }

    #[test]
    fn test_comparison_boolean() {
        let expr = binary(num_literal("1", 0), BinaryOp::Lt, num_literal("2", 2), Span::new(0, 3));
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("boolean".to_string()));
    }

    #[test]
    fn test_concat_text() {
        let expr = Expression::Binary(BinaryExpr {
            left: Box::new(str_literal("hello", 0)),
            operator: BinaryOp::Concat,
            right: Box::new(str_literal("world", 7)),
            span: Span::new(0, 13),
        });
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("text".to_string()));
    }

    #[test]
    fn test_unknown_expression() {
        let builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.inference.get_type(999);
        assert_eq!(ty, &TypeInfo::Unknown);
    }

    #[test]
    fn test_function_call_undeclared() {
        let expr = function_call("unknown_func", vec![], 0, 13);
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Unknown);
    }

    #[test]
    fn test_property_access_known() {
        let expr = Expression::PropertyAccess(PropertyAccessExpr {
            object: Box::new(variable("player", 0)),
            property: "health".into(),
            span: Span::new(0, 13),
        });
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("number".to_string()));
    }

    #[test]
    fn test_property_access_unknown() {
        let expr = Expression::PropertyAccess(PropertyAccessExpr {
            object: Box::new(variable("obj", 0)),
            property: "customField".into(),
            span: Span::new(0, 14),
        });
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Any);
    }

    #[test]
    fn test_string_interpolation() {
        let s = Span::new(0, 10);
        let expr = Expression::StringInterpolation(StrInterpolationExpr {
            parts: vec![
                StrPart::Text("hello ".into()),
                StrPart::Interpolation(variable("name", 6)),
            ],
            span: s,
        });
        let mut builder = TypeInferenceBuilder::new(ScopeTree::new(), FunctionRegistry::new());
        let ty = builder.infer_expression(&expr);
        assert_eq!(ty, TypeInfo::Known("text".to_string()));
    }

    fn binary(left: Expression, op: BinaryOp, right: Expression, span: Span) -> Expression {
        Expression::Binary(BinaryExpr {
            left: Box::new(left),
            operator: op,
            right: Box::new(right),
            span,
        })
    }
}