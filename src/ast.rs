/// A byte-offset span in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

/// A unique node identifier for arena allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn new(id: u32) -> Self {
        NodeId(id)
    }
}

// ---------------------------------------------------------------------------
// Program & top-level items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<TopLevelItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevelItem {
    Function(FunctionDecl),
    Command(CommandDecl),
    Event(EventDecl),
    PeriodicEvent(PeriodicEventDecl),
    Options(OptionsBlock),
    Aliases(AliasesBlock),
    Variables(VariablesBlock),
    Using(UsingDirective),
    Example(ExampleBlock),
    Comment(Comment),
    BlankLine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandDecl {
    pub name: String,
    pub arguments: Vec<CommandArg>,
    pub metadata: CommandMetadata,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDecl {
    pub event_type: String,
    pub conditions: Vec<Condition>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeriodicEventDecl {
    pub period: String,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionsBlock {
    pub entries: Vec<OptionEntry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AliasesBlock {
    pub entries: Vec<AliasEntry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariablesBlock {
    pub entries: Vec<VariableDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsingDirective {
    pub what: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExampleBlock {
    pub content: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub text: String,
    pub span: Span,
}

// Supporting types for top-level items

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub type_: Option<Type>,
    pub default: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandArg {
    pub name: String,
    pub type_: Option<Type>,
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandMetadata {
    pub description: Option<String>,
    pub executable_by: Option<String>,
    pub permission: Option<String>,
    pub permission_message: Option<String>,
    pub cooldown: Option<String>,
    pub cooldown_message: Option<String>,
    pub cooldown_bypass: Option<String>,
    pub aliases: Option<String>,
    pub usage: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionEntry {
    pub key: String,
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AliasEntry {
    pub source: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub expression: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableDecl {
    pub name: String,
    pub type_: Option<Type>,
    pub initializer: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub name: String,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Statement types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    If(IfStmt),
    ElseIf(ElseIfStmt),
    Else(ElseStmt),
    Loop(LoopStmt),
    While(WhileStmt),
    DoWhile(DoWhileStmt),
    ForIn(ForInStmt),
    Assignment(AssignmentStmt),
    Effect(EffectStmt),
    Return(ReturnStmt),
    Stop(StopStmt),
    Exit(ExitStmt),
    Continue(ContinueStmt),
    CancelEvent(CancelEventStmt),
    Wait(WaitStmt),
    Expression(Expression),
    Comment(Comment),
    BlankLine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub condition: Expression,
    pub body: Vec<Statement>,
    pub else_if: Vec<ElseIfStmt>,
    pub else_branch: Option<Box<ElseStmt>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElseIfStmt {
    pub condition: Expression,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElseStmt {
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoopStmt {
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStmt {
    pub condition: Expression,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoWhileStmt {
    pub body: Vec<Statement>,
    pub condition: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForInStmt {
    pub variable: String,
    pub iterable: Expression,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentStmt {
    pub target: AssignmentTarget,
    pub value: Expression,
    pub operator: AssignmentOp,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectStmt {
    pub effect: String,
    pub arguments: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStmt {
    pub value: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StopStmt {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExitStmt {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinueStmt {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelEventStmt {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaitStmt {
    pub duration: Option<Expression>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Expression types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Ternary(TernaryExpr),
    PropertyAccess(PropertyAccessExpr),
    FunctionCall(FunctionCallExpr),
    Index(IndexExpr),
    Literal(Literal),
    Variable(Variable),
    EventValue(EventValueExpr),
    List(ListExpr),
    Parenthesized(ParenthesizedExpr),
    StringInterpolation(StrInterpolationExpr),
    Unknown(UnknownExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpr {
    pub left: Box<Expression>,
    pub operator: BinaryOp,
    pub right: Box<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnaryExpr {
    pub operator: UnaryOp,
    pub operand: Box<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TernaryExpr {
    pub condition: Box<Expression>,
    pub then_branch: Box<Expression>,
    pub else_branch: Box<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAccessExpr {
    pub object: Box<Expression>,
    pub property: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallExpr {
    pub callee: Box<Expression>,
    pub arguments: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexExpr {
    pub target: Box<Expression>,
    pub index: Box<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventValueExpr {
    pub type_name: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListExpr {
    pub elements: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParenthesizedExpr {
    pub expression: Box<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrInterpolationExpr {
    pub parts: Vec<StrPart>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnknownExpr {
    pub content: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Number(NumberLiteral),
    String(StringLiteral),
    Boolean(BooleanLiteral),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NumberLiteral {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringLiteral {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BooleanLiteral {
    pub value: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Text(String),
    Interpolation(Expression),
}

// ---------------------------------------------------------------------------
// Operator enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    And,
    Or,
    Eq,
    Neq,
    Lt,
    Gt,
    Leq,
    Geq,
    In,
    InstanceOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignmentOp {
    Equal,
    ColonEqual,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignmentTarget {
    Variable(Variable),
    PropertyAccess(PropertyAccessExpr),
    Index(IndexExpr),
}

// ---------------------------------------------------------------------------
// Display trait implementations
// ---------------------------------------------------------------------------

use std::fmt;

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Mod => write!(f, "%"),
            BinaryOp::Pow => write!(f, "^"),
            BinaryOp::Concat => write!(f, "~"),
            BinaryOp::And => write!(f, "&&"),
            BinaryOp::Or => write!(f, "||"),
            BinaryOp::Eq => write!(f, "=="),
            BinaryOp::Neq => write!(f, "!="),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::Leq => write!(f, "<="),
            BinaryOp::Geq => write!(f, ">="),
            BinaryOp::In => write!(f, "in"),
            BinaryOp::InstanceOf => write!(f, "instanceof"),
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::Length => write!(f, "#"),
        }
    }
}

impl fmt::Display for AssignmentOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssignmentOp::Equal => write!(f, "="),
            AssignmentOp::ColonEqual => write!(f, ":="),
            AssignmentOp::PlusEqual => write!(f, "+="),
            AssignmentOp::MinusEqual => write!(f, "-="),
            AssignmentOp::StarEqual => write!(f, "*="),
            AssignmentOp::SlashEqual => write!(f, "/="),
        }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Number(n) => write!(f, "{}", n.value),
            Literal::String(s) => write!(f, "\"{}\"", s.value),
            Literal::Boolean(b) => write!(f, "{}", b.value),
        }
    }
}

impl fmt::Display for AssignmentTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssignmentTarget::Variable(v) => write!(f, "{}", v.name),
            AssignmentTarget::PropertyAccess(p) => write!(f, "{}.{}", p.object, p.property),
            AssignmentTarget::Index(i) => write!(f, "{}[{}]", i.target, i.index),
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Binary(_) => write!(f, "<binary>"),
            Expression::Unary(_) => write!(f, "<unary>"),
            Expression::Ternary(_) => write!(f, "<ternary>"),
            Expression::PropertyAccess(_) => write!(f, "<property_access>"),
            Expression::FunctionCall(_) => write!(f, "<function_call>"),
            Expression::Index(_) => write!(f, "<index>"),
            Expression::Literal(lit) => fmt::Display::fmt(lit, f),
            Expression::Variable(v) => write!(f, "{}", v.name),
            Expression::EventValue(_) => write!(f, "<event_value>"),
            Expression::List(_) => write!(f, "<list>"),
            Expression::Parenthesized(_) => write!(f, "<parenthesized>"),
            Expression::StringInterpolation(_) => write!(f, "<str_interpolation>"),
            Expression::Unknown(e) => write!(f, "<unknown: {}>", e.content),
        }
    }
}

// ---------------------------------------------------------------------------
// Visitor trait
// ---------------------------------------------------------------------------

pub trait Visitor {
    fn visit_program(&mut self, program: &Program) {
        walk_program(self, program);
    }

    fn visit_top_level_item(&mut self, item: &TopLevelItem) {
        walk_top_level_item(self, item);
    }

    fn visit_statement(&mut self, statement: &Statement) {
        walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &Expression) {
        walk_expression(self, expression);
    }

    fn visit_parameter(&mut self, _parameter: &Parameter) {}
    fn visit_command_arg(&mut self, _arg: &CommandArg) {}
    fn visit_condition(&mut self, condition: &Condition) {
        self.visit_expression(&condition.expression);
    }
    fn visit_option_entry(&mut self, _entry: &OptionEntry) {}
    fn visit_alias_entry(&mut self, _entry: &AliasEntry) {}
    fn visit_variable_decl(&mut self, _decl: &VariableDecl) {}
    fn visit_comment(&mut self, _comment: &Comment) {}
    fn visit_type(&mut self, _type: &Type) {}
    fn visit_literal(&mut self, _literal: &Literal) {}
    fn visit_str_part(&mut self, part: &StrPart) {
        if let StrPart::Interpolation(expr) = part {
            self.visit_expression(expr);
        }
    }
    fn visit_assignment_target(&mut self, target: &AssignmentTarget) {
        walk_assignment_target(self, target);
    }
}

pub fn walk_program<V: Visitor + ?Sized>(visitor: &mut V, program: &Program) {
    for item in &program.items {
        visitor.visit_top_level_item(item);
    }
}

pub fn walk_top_level_item<V: Visitor + ?Sized>(visitor: &mut V, item: &TopLevelItem) {
    match item {
        TopLevelItem::Function(f) => {
            for p in &f.parameters {
                visitor.visit_parameter(p);
            }
            if let Some(ref t) = f.return_type {
                visitor.visit_type(t);
            }
            for s in &f.body {
                visitor.visit_statement(s);
            }
        }
        TopLevelItem::Command(c) => {
            for a in &c.arguments {
                visitor.visit_command_arg(a);
            }
            for s in &c.body {
                visitor.visit_statement(s);
            }
        }
        TopLevelItem::Event(e) => {
            for c in &e.conditions {
                visitor.visit_condition(c);
            }
            for s in &e.body {
                visitor.visit_statement(s);
            }
        }
        TopLevelItem::PeriodicEvent(p) => {
            for s in &p.body {
                visitor.visit_statement(s);
            }
        }
        TopLevelItem::Options(o) => {
            for e in &o.entries {
                visitor.visit_option_entry(e);
            }
        }
        TopLevelItem::Aliases(a) => {
            for e in &a.entries {
                visitor.visit_alias_entry(e);
            }
        }
        TopLevelItem::Variables(v) => {
            for e in &v.entries {
                visitor.visit_variable_decl(e);
            }
        }
        TopLevelItem::Using(_) => {}
        TopLevelItem::Example(e) => {
            for s in &e.content {
                visitor.visit_statement(s);
            }
        }
        TopLevelItem::Comment(c) => {
            visitor.visit_comment(c);
        }
        TopLevelItem::BlankLine => {}
    }
}

pub fn walk_statement<V: Visitor + ?Sized>(visitor: &mut V, statement: &Statement) {
    match statement {
        Statement::If(s) => {
            visitor.visit_expression(&s.condition);
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
            for elif in &s.else_if {
                visitor.visit_expression(&elif.condition);
                for stmt in &elif.body {
                    visitor.visit_statement(stmt);
                }
            }
            if let Some(ref el) = s.else_branch {
                for stmt in &el.body {
                    visitor.visit_statement(stmt);
                }
            }
        }
        Statement::ElseIf(s) => {
            visitor.visit_expression(&s.condition);
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
        }
        Statement::Else(s) => {
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
        }
        Statement::Loop(s) => {
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
        }
        Statement::While(s) => {
            visitor.visit_expression(&s.condition);
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
        }
        Statement::DoWhile(s) => {
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
            visitor.visit_expression(&s.condition);
        }
        Statement::ForIn(s) => {
            visitor.visit_expression(&s.iterable);
            for stmt in &s.body {
                visitor.visit_statement(stmt);
            }
        }
        Statement::Assignment(s) => {
            visitor.visit_assignment_target(&s.target);
            visitor.visit_expression(&s.value);
        }
        Statement::Effect(s) => {
            for arg in &s.arguments {
                visitor.visit_expression(arg);
            }
        }
        Statement::Return(s) => {
            if let Some(ref val) = s.value {
                visitor.visit_expression(val);
            }
        }
        Statement::Stop(_) => {}
        Statement::Exit(_) => {}
        Statement::Continue(_) => {}
        Statement::CancelEvent(_) => {}
        Statement::Wait(s) => {
            if let Some(ref dur) = s.duration {
                visitor.visit_expression(dur);
            }
        }
        Statement::Expression(expr) => {
            visitor.visit_expression(expr);
        }
        Statement::Comment(c) => {
            visitor.visit_comment(c);
        }
        Statement::BlankLine => {}
    }
}

pub fn walk_expression<V: Visitor + ?Sized>(visitor: &mut V, expression: &Expression) {
    match expression {
        Expression::Binary(e) => {
            visitor.visit_expression(&e.left);
            visitor.visit_expression(&e.right);
        }
        Expression::Unary(e) => {
            visitor.visit_expression(&e.operand);
        }
        Expression::Ternary(e) => {
            visitor.visit_expression(&e.condition);
            visitor.visit_expression(&e.then_branch);
            visitor.visit_expression(&e.else_branch);
        }
        Expression::PropertyAccess(e) => {
            visitor.visit_expression(&e.object);
        }
        Expression::FunctionCall(e) => {
            visitor.visit_expression(&e.callee);
            for arg in &e.arguments {
                visitor.visit_expression(arg);
            }
        }
        Expression::Index(e) => {
            visitor.visit_expression(&e.target);
            visitor.visit_expression(&e.index);
        }
        Expression::Literal(lit) => {
            visitor.visit_literal(lit);
        }
        Expression::Variable(_) => {}
        Expression::EventValue(_) => {}
        Expression::List(e) => {
            for elem in &e.elements {
                visitor.visit_expression(elem);
            }
        }
        Expression::Parenthesized(e) => {
            visitor.visit_expression(&e.expression);
        }
        Expression::StringInterpolation(e) => {
            for part in &e.parts {
                visitor.visit_str_part(part);
            }
        }
        Expression::Unknown(_) => {}
    }
}

pub fn walk_assignment_target<V: Visitor + ?Sized>(visitor: &mut V, target: &AssignmentTarget) {
    match target {
        AssignmentTarget::Variable(_) => {}
        AssignmentTarget::PropertyAccess(p) => {
            visitor.visit_expression(&p.object);
        }
        AssignmentTarget::Index(i) => {
            visitor.visit_expression(&i.target);
            visitor.visit_expression(&i.index);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let s = Span::new(5, 10);
        assert_eq!(s.start, 5);
        assert_eq!(s.end, 10);
        let s2 = Span::new(0, 0);
        assert_eq!(s2.start, 0);
        assert_eq!(s2.end, 0);
    }

    #[test]
    fn test_node_id() {
        let a = NodeId::new(42);
        let b = NodeId::new(42);
        let c = NodeId::new(0);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.0, 42);
    }

    #[test]
    fn test_operator_display() {
        assert_eq!(BinaryOp::Add.to_string(), "+");
        assert_eq!(BinaryOp::Sub.to_string(), "-");
        assert_eq!(BinaryOp::Mul.to_string(), "*");
        assert_eq!(BinaryOp::Div.to_string(), "/");
        assert_eq!(BinaryOp::Mod.to_string(), "%");
        assert_eq!(BinaryOp::Pow.to_string(), "^");
        assert_eq!(BinaryOp::Concat.to_string(), "~");
        assert_eq!(BinaryOp::And.to_string(), "&&");
        assert_eq!(BinaryOp::Or.to_string(), "||");
        assert_eq!(BinaryOp::Eq.to_string(), "==");
        assert_eq!(BinaryOp::Neq.to_string(), "!=");
        assert_eq!(BinaryOp::Lt.to_string(), "<");
        assert_eq!(BinaryOp::Gt.to_string(), ">");
        assert_eq!(BinaryOp::Leq.to_string(), "<=");
        assert_eq!(BinaryOp::Geq.to_string(), ">=");
        assert_eq!(BinaryOp::In.to_string(), "in");
        assert_eq!(BinaryOp::InstanceOf.to_string(), "instanceof");

        assert_eq!(UnaryOp::Neg.to_string(), "-");
        assert_eq!(UnaryOp::Not.to_string(), "!");
        assert_eq!(UnaryOp::Length.to_string(), "#");

        assert_eq!(AssignmentOp::Equal.to_string(), "=");
        assert_eq!(AssignmentOp::ColonEqual.to_string(), ":=");
        assert_eq!(AssignmentOp::PlusEqual.to_string(), "+=");
        assert_eq!(AssignmentOp::MinusEqual.to_string(), "-=");
        assert_eq!(AssignmentOp::StarEqual.to_string(), "*=");
        assert_eq!(AssignmentOp::SlashEqual.to_string(), "/=");
    }

    #[test]
    fn test_literal_display() {
        let n = Literal::Number(NumberLiteral { value: "42".into(), span: Span::new(0, 2) });
        assert_eq!(n.to_string(), "42");

        let s = Literal::String(StringLiteral { value: "hello".into(), span: Span::new(0, 7) });
        assert_eq!(s.to_string(), "\"hello\"");

        let bt = Literal::Boolean(BooleanLiteral { value: true, span: Span::new(0, 4) });
        assert_eq!(bt.to_string(), "true");

        let bf = Literal::Boolean(BooleanLiteral { value: false, span: Span::new(0, 5) });
        assert_eq!(bf.to_string(), "false");
    }

    #[test]
    fn test_expression_construction() {
        let s = Span::new(0, 1);

        let _binary = Expression::Binary(BinaryExpr {
            left: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "1".into(),
                span: s,
            }))),
            operator: BinaryOp::Add,
            right: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "2".into(),
                span: s,
            }))),
            span: s,
        });

        let _unary = Expression::Unary(UnaryExpr {
            operator: UnaryOp::Neg,
            operand: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "5".into(),
                span: s,
            }))),
            span: s,
        });

        let _ternary = Expression::Ternary(TernaryExpr {
            condition: Box::new(Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: true,
                span: s,
            }))),
            then_branch: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "1".into(),
                span: s,
            }))),
            else_branch: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "2".into(),
                span: s,
            }))),
            span: s,
        });

        let _prop = Expression::PropertyAccess(PropertyAccessExpr {
            object: Box::new(Expression::Variable(Variable {
                name: "obj".into(),
                span: s,
            })),
            property: "field".into(),
            span: s,
        });

        let _call = Expression::FunctionCall(FunctionCallExpr {
            callee: Box::new(Expression::Variable(Variable {
                name: "f".into(),
                span: s,
            })),
            arguments: vec![],
            span: s,
        });

        let _index = Expression::Index(IndexExpr {
            target: Box::new(Expression::Variable(Variable {
                name: "arr".into(),
                span: s,
            })),
            index: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "0".into(),
                span: s,
            }))),
            span: s,
        });

        let _lit = Expression::Literal(Literal::Number(NumberLiteral {
            value: "42".into(),
            span: s,
        }));

        let _var = Expression::Variable(Variable {
            name: "var".into(),
            span: s,
        });

        let _ev = Expression::EventValue(EventValueExpr {
            type_name: None,
            span: s,
        });

        let _list = Expression::List(ListExpr {
            elements: vec![],
            span: s,
        });

        let _paren = Expression::Parenthesized(ParenthesizedExpr {
            expression: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "1".into(),
                span: s,
            }))),
            span: s,
        });

        let _interp = Expression::StringInterpolation(StrInterpolationExpr {
            parts: vec![
                StrPart::Text("hello ".into()),
                StrPart::Interpolation(Expression::Variable(Variable {
                    name: "name".into(),
                    span: s,
                })),
            ],
            span: s,
        });

        let _unknown = Expression::Unknown(UnknownExpr {
            content: "???".into(),
            span: s,
        });

        // Debug shouldn't panic
        let _ = format!("{:?}", _binary);
    }

    #[test]
    fn test_statement_construction() {
        let s = Span::new(0, 1);

        let _if = Statement::If(IfStmt {
            condition: Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: true,
                span: s,
            })),
            body: vec![],
            else_if: vec![],
            else_branch: None,
            span: s,
        });

        let _else_if = Statement::ElseIf(ElseIfStmt {
            condition: Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: false,
                span: s,
            })),
            body: vec![],
            span: s,
        });

        let _else = Statement::Else(ElseStmt {
            body: vec![],
            span: s,
        });

        let _loop = Statement::Loop(LoopStmt {
            body: vec![],
            span: s,
        });

        let _while = Statement::While(WhileStmt {
            condition: Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: true,
                span: s,
            })),
            body: vec![],
            span: s,
        });

        let _do_while = Statement::DoWhile(DoWhileStmt {
            body: vec![],
            condition: Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: true,
                span: s,
            })),
            span: s,
        });

        let _for_in = Statement::ForIn(ForInStmt {
            variable: "i".into(),
            iterable: Expression::Variable(Variable {
                name: "list".into(),
                span: s,
            }),
            body: vec![],
            span: s,
        });

        let _assign = Statement::Assignment(AssignmentStmt {
            target: AssignmentTarget::Variable(Variable {
                name: "x".into(),
                span: s,
            }),
            value: Expression::Literal(Literal::Number(NumberLiteral {
                value: "1".into(),
                span: s,
            })),
            operator: AssignmentOp::Equal,
            span: s,
        });

        let _effect = Statement::Effect(EffectStmt {
            effect: "send".into(),
            arguments: vec![],
            span: s,
        });

        let _return = Statement::Return(ReturnStmt {
            value: None,
            span: s,
        });

        let _stop = Statement::Stop(StopStmt { span: s });
        let _exit = Statement::Exit(ExitStmt { span: s });
        let _continue = Statement::Continue(ContinueStmt { span: s });
        let _cancel = Statement::CancelEvent(CancelEventStmt { span: s });

        let _wait = Statement::Wait(WaitStmt {
            duration: None,
            span: s,
        });

        let _expr = Statement::Expression(Expression::Literal(Literal::Number(NumberLiteral {
            value: "1".into(),
            span: s,
        })));

        let _comment = Statement::Comment(Comment {
            text: "# hi".into(),
            span: s,
        });

        let _blank = Statement::BlankLine;

        // Debug shouldn't panic
        let _ = format!("{:?}", _if);
    }

    #[test]
    fn test_top_level_construction() {
        let s = Span::new(0, 1);

        let _func = TopLevelItem::Function(FunctionDecl {
            name: "doStuff".into(),
            parameters: vec![],
            return_type: None,
            body: vec![],
            span: s,
        });

        let _cmd = TopLevelItem::Command(CommandDecl {
            name: "/hello".into(),
            arguments: vec![],
            metadata: CommandMetadata::default(),
            body: vec![],
            span: s,
        });

        let _event = TopLevelItem::Event(EventDecl {
            event_type: "on join".into(),
            conditions: vec![],
            body: vec![],
            span: s,
        });

        let _periodic = TopLevelItem::PeriodicEvent(PeriodicEventDecl {
            period: "every 5 seconds".into(),
            body: vec![],
            span: s,
        });

        let _options = TopLevelItem::Options(OptionsBlock {
            entries: vec![],
            span: s,
        });

        let _aliases = TopLevelItem::Aliases(AliasesBlock {
            entries: vec![],
            span: s,
        });

        let _variables = TopLevelItem::Variables(VariablesBlock {
            entries: vec![],
            span: s,
        });

        let _using = TopLevelItem::Using(UsingDirective {
            what: "skript".into(),
            span: s,
        });

        let _example = TopLevelItem::Example(ExampleBlock {
            content: vec![],
            span: s,
        });

        let _comment = TopLevelItem::Comment(Comment {
            text: "# comment".into(),
            span: s,
        });

        let _blank = TopLevelItem::BlankLine;

        // Debug shouldn't panic
        let _ = format!("{:?}", _func);
    }

    struct CountingVisitor {
        program_count: usize,
        top_level_count: usize,
        statement_count: usize,
        expression_count: usize,
        parameter_count: usize,
        command_arg_count: usize,
        condition_count: usize,
        option_entry_count: usize,
        alias_entry_count: usize,
        variable_decl_count: usize,
        comment_count: usize,
        literal_count: usize,
    }

    impl CountingVisitor {
        fn new() -> Self {
            CountingVisitor {
                program_count: 0,
                top_level_count: 0,
                statement_count: 0,
                expression_count: 0,
                parameter_count: 0,
                command_arg_count: 0,
                condition_count: 0,
                option_entry_count: 0,
                alias_entry_count: 0,
                variable_decl_count: 0,
                comment_count: 0,
                literal_count: 0,
            }
        }
    }

    impl Visitor for CountingVisitor {
        fn visit_program(&mut self, program: &Program) {
            self.program_count += 1;
            walk_program(self, program);
        }

        fn visit_top_level_item(&mut self, item: &TopLevelItem) {
            self.top_level_count += 1;
            walk_top_level_item(self, item);
        }

        fn visit_statement(&mut self, statement: &Statement) {
            self.statement_count += 1;
            walk_statement(self, statement);
        }

        fn visit_expression(&mut self, expression: &Expression) {
            self.expression_count += 1;
            walk_expression(self, expression);
        }

        fn visit_parameter(&mut self, _parameter: &Parameter) {
            self.parameter_count += 1;
        }

        fn visit_command_arg(&mut self, _arg: &CommandArg) {
            self.command_arg_count += 1;
        }

        fn visit_condition(&mut self, condition: &Condition) {
            self.condition_count += 1;
            self.visit_expression(&condition.expression);
        }

        fn visit_option_entry(&mut self, _entry: &OptionEntry) {
            self.option_entry_count += 1;
        }

        fn visit_alias_entry(&mut self, _entry: &AliasEntry) {
            self.alias_entry_count += 1;
        }

        fn visit_variable_decl(&mut self, _decl: &VariableDecl) {
            self.variable_decl_count += 1;
        }

        fn visit_comment(&mut self, _comment: &Comment) {
            self.comment_count += 1;
        }

        fn visit_literal(&mut self, _literal: &Literal) {
            self.literal_count += 1;
        }
    }

    #[test]
    fn test_visitor_basic() {
        let s = Span::new(0, 1);

        let program = Program {
            items: vec![
                TopLevelItem::Function(FunctionDecl {
                    name: "test".into(),
                    parameters: vec![Parameter {
                        name: "x".into(),
                        type_: None,
                        default: None,
                        span: s,
                    }],
                    return_type: None,
                    body: vec![
                        Statement::Expression(Expression::Literal(Literal::Number(
                            NumberLiteral { value: "1".into(), span: s },
                        ))),
                        Statement::Return(ReturnStmt {
                            value: Some(Expression::Variable(Variable {
                                name: "x".into(),
                                span: s,
                            })),
                            span: s,
                        }),
                    ],
                    span: s,
                }),
                TopLevelItem::Command(CommandDecl {
                    name: "/greet".into(),
                    arguments: vec![CommandArg {
                        name: "player".into(),
                        type_: None,
                        optional: false,
                        span: s,
                    }],
                    metadata: CommandMetadata::default(),
                    body: vec![
                        Statement::Effect(EffectStmt {
                            effect: "send".into(),
                            arguments: vec![Expression::Literal(Literal::String(
                                StringLiteral { value: "hi".into(), span: s },
                            ))],
                            span: s,
                        }),
                    ],
                    span: s,
                }),
                TopLevelItem::Comment(Comment {
                    text: "# header".into(),
                    span: s,
                }),
                TopLevelItem::BlankLine,
            ],
        };

        let mut visitor = CountingVisitor::new();
        visitor.visit_program(&program);

        assert_eq!(visitor.program_count, 1, "should visit program once");
        assert_eq!(visitor.top_level_count, 4, "4 top-level items");
        assert_eq!(visitor.statement_count, 3, "2 statements in func + 1 effect in cmd");
        assert_eq!(visitor.expression_count, 3, "num(1) + var(x) + string(hi)");
        assert_eq!(visitor.parameter_count, 1, "1 parameter");
        assert_eq!(visitor.command_arg_count, 1, "1 command arg");
        assert_eq!(visitor.comment_count, 1, "1 comment");
        assert_eq!(visitor.literal_count, 2, "num(1) + string(hi)");
    }

    #[test]
    fn test_assignment_target() {
        let s = Span::new(0, 1);

        let var_target = AssignmentTarget::Variable(Variable {
            name: "x".into(),
            span: s,
        });
        assert_eq!(var_target.to_string(), "x");

        let prop_target = AssignmentTarget::PropertyAccess(PropertyAccessExpr {
            object: Box::new(Expression::Variable(Variable {
                name: "obj".into(),
                span: s,
            })),
            property: "field".into(),
            span: s,
        });
        assert_eq!(prop_target.to_string(), "obj.field");

        let index_target = AssignmentTarget::Index(IndexExpr {
            target: Box::new(Expression::Variable(Variable {
                name: "arr".into(),
                span: s,
            })),
            index: Box::new(Expression::Literal(Literal::Number(NumberLiteral {
                value: "0".into(),
                span: s,
            }))),
            span: s,
        });
        assert_eq!(index_target.to_string(), "arr[0]");
    }
}
