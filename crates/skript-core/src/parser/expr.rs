use crate::ast::*;
use crate::lexer::Token;
use crate::parser::Parser;

/// Parse an expression from the current position of the parser.
///
/// This is the entry point for the Pratt (top-down operator precedence)
/// expression parser.
pub fn parse_expression(parser: &mut Parser) -> Expression {
    parse_precedence(parser, 0)
}

// ---------------------------------------------------------------------------
// Precedence table
// ---------------------------------------------------------------------------

/// Return the (precedence, left_associative) pair for a token, or `None` if
/// the token is not a binary operator.
fn get_precedence(token: &Token) -> Option<(u8, bool)> {
    match token {
        Token::Keyword("or") => Some((1, true)),
        Token::Keyword("and") => Some((2, true)),
        Token::Keyword("is") | Token::Keyword("isn't") | Token::EqualEqual | Token::NotEqual => {
            Some((3, true))
        }
        Token::Less | Token::Greater | Token::LessEqual | Token::GreaterEqual => Some((4, true)),
        Token::Keyword("in") | Token::Keyword("instanceof") => Some((5, true)),
        Token::Identifier(s) if s == "instanceof" => Some((5, true)),
        Token::Tilde => Some((6, true)),
        Token::Plus | Token::Minus => Some((7, true)),
        Token::Star | Token::Slash | Token::Percent => Some((8, true)),
        Token::Caret => Some((9, false)), // right-associative
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Token → operator mapping
// ---------------------------------------------------------------------------

fn token_to_binary_op(token: &Token) -> BinaryOp {
    match token {
        Token::Plus => BinaryOp::Add,
        Token::Minus => BinaryOp::Sub,
        Token::Star => BinaryOp::Mul,
        Token::Slash => BinaryOp::Div,
        Token::Percent => BinaryOp::Mod,
        Token::Caret => BinaryOp::Pow,
        Token::Tilde => BinaryOp::Concat,
        Token::EqualEqual => BinaryOp::Eq,
        Token::NotEqual => BinaryOp::Neq,
        Token::Less => BinaryOp::Lt,
        Token::Greater => BinaryOp::Gt,
        Token::LessEqual => BinaryOp::Leq,
        Token::GreaterEqual => BinaryOp::Geq,
        Token::Keyword("and") => BinaryOp::And,
        Token::Keyword("or") => BinaryOp::Or,
        Token::Keyword("is") => BinaryOp::Eq,
        Token::Keyword("isn't") => BinaryOp::Neq,
        Token::Keyword("in") => BinaryOp::In,
        Token::Keyword("instanceof") => BinaryOp::InstanceOf,
        Token::Identifier(s) if s == "instanceof" => BinaryOp::InstanceOf,
        _ => unreachable!("token_to_binary_op: unexpected token {:?}", token),
    }
}

// ---------------------------------------------------------------------------
// Span helpers
// ---------------------------------------------------------------------------

fn span_of(expr: &Expression) -> Span {
    match expr {
        Expression::Binary(e) => e.span,
        Expression::Unary(e) => e.span,
        Expression::Ternary(e) => e.span,
        Expression::PropertyAccess(e) => e.span,
        Expression::FunctionCall(e) => e.span,
        Expression::Index(e) => e.span,
        Expression::Literal(l) => match l {
            Literal::Number(n) => n.span,
            Literal::String(s) => s.span,
            Literal::Boolean(b) => b.span,
        },
        Expression::Variable(v) => v.span,
        Expression::EventValue(e) => e.span,
        Expression::List(l) => l.span,
        Expression::Parenthesized(p) => p.span,
        Expression::StringInterpolation(s) => s.span,
        Expression::Unknown(u) => u.span,
    }
}

fn combine_spans(left: &Expression, right: &Expression) -> Span {
    Span::new(span_of(left).start, span_of(right).end)
}

// ---------------------------------------------------------------------------
// Helper: expect identifier or keyword
// ---------------------------------------------------------------------------

/// Consume an `Identifier` or `Keyword` token and return its text.
fn expect_identifier_or_keyword(parser: &mut Parser) -> String {
    let tok = parser.peek();
    let text = match &tok.token {
        Token::Identifier(s) => s.clone(),
        Token::Keyword(s) => s.to_string(),
        _ => {
            parser.error(
                format!("Expected identifier or keyword, got {:?}", tok.token),
                tok.span,
            );
            return String::new();
        }
    };
    parser.advance();
    text
}

// ---------------------------------------------------------------------------
// Postfix parsers
// ---------------------------------------------------------------------------

/// Parse a function call: `callee(args...)`.
fn parse_function_call(parser: &mut Parser, callee: Expression) -> Expression {
    let callee_span = span_of(&callee);
    parser.advance(); // consume LParen

    let mut arguments = Vec::new();

    if !matches!(parser.peek_token(), Token::RParen) {
        loop {
            arguments.push(parse_expression(parser));
            if !parser.consume_if(&Token::Comma) {
                break;
            }
        }
    }

    let close_span = parser.peek().span;
    parser.expect(&Token::RParen);

    Expression::FunctionCall(FunctionCallExpr {
        callee: Box::new(callee),
        arguments,
        span: Span::new(callee_span.start, close_span.end),
    })
}

/// Parse an index expression: `target[index]`.
fn parse_index(parser: &mut Parser, target: Expression) -> Expression {
    let target_span = span_of(&target);
    parser.advance(); // consume LBracket

    let index = parse_expression(parser);

    let close_span = parser.peek().span;
    parser.expect(&Token::RBracket);

    Expression::Index(IndexExpr {
        target: Box::new(target),
        index: Box::new(index),
        span: Span::new(target_span.start, close_span.end),
    })
}

/// Parse a parenthesized expression or comma-separated list.
///
/// After consuming the opening `(`, this function decides based on what
/// follows:
/// - `)` → empty list
/// - `expr, ...` → list expression
/// - `expr)` → parenthesized expression
fn parse_parenthesized_or_list(parser: &mut Parser) -> Expression {
    let open_span = parser.peek().span;

    // Empty list: ()
    if parser.consume_if(&Token::RParen) {
        return Expression::List(ListExpr {
            elements: vec![],
            span: Span::new(open_span.start, open_span.end),
        });
    }

    let first = parse_expression(parser);

    if parser.consume_if(&Token::Comma) {
        let mut elements = vec![first];
        while !matches!(parser.peek_token(), Token::RParen) {
            elements.push(parse_expression(parser));
            if !parser.consume_if(&Token::Comma) {
                break;
            }
        }
        let close_span = parser.peek().span;
        parser.expect(&Token::RParen);
        Expression::List(ListExpr {
            elements,
            span: Span::new(open_span.start, close_span.end),
        })
    } else {
        let close_span = parser.peek().span;
        parser.expect(&Token::RParen);
        Expression::Parenthesized(ParenthesizedExpr {
            expression: Box::new(first),
            span: Span::new(open_span.start, close_span.end),
        })
    }
}

/// Parse a string interpolation / MiniMessage sequence.
///
/// Collects a run of `StrText`, `StrInterpolationStart`/`End`,
/// `MiniMessageOpen`/`Close` tokens into a single
/// [`StrInterpolationExpr`].
fn parse_interpolation(parser: &mut Parser) -> Expression {
    let start = parser.peek().span.start;
    let mut parts = Vec::new();

    loop {
        match parser.peek_token() {
            Token::StrText(_) => {
                let tok = parser.advance();
                if let Token::StrText(s) = tok.token {
                    parts.push(StrPart::Text(s));
                }
            }
            Token::StrInterpolationStart => {
                parser.advance();
                let expr = parse_expression(parser);
                parts.push(StrPart::Interpolation(expr));
                parser.expect(&Token::StrInterpolationEnd);
            }
            Token::MiniMessageOpen(tag_name) => {
                parser.advance();
                parts.push(StrPart::Text(format!("<{}>", tag_name)));
            }
            Token::MiniMessageClose(tag_name) => {
                parser.advance();
                parts.push(StrPart::Text(format!("</{}>", tag_name)));
            }
            _ => break,
        }
    }

    Expression::StringInterpolation(StrInterpolationExpr {
        parts,
        span: Span::new(start, parser.peek().span.start),
    })
}

// ---------------------------------------------------------------------------
// Prefix / primary expression parser
// ---------------------------------------------------------------------------

fn parse_prefix(parser: &mut Parser) -> Expression {
    let tok = parser.peek();
    let span = tok.span;

    match tok.token {
        Token::Number(val) => {
            parser.advance();
            Expression::Literal(Literal::Number(NumberLiteral { value: val, span }))
        }
        Token::String(val) => {
            parser.advance();
            Expression::Literal(Literal::String(StringLiteral { value: val, span }))
        }
        Token::Boolean(val) => {
            parser.advance();
            Expression::Literal(Literal::Boolean(BooleanLiteral { value: val, span }))
        }
        Token::Keyword("true") => {
            parser.advance();
            Expression::Literal(Literal::Boolean(BooleanLiteral { value: true, span }))
        }
        Token::Keyword("false") => {
            parser.advance();
            Expression::Literal(Literal::Boolean(BooleanLiteral { value: false, span }))
        }
        Token::Keyword("event") => {
            parser.advance();
            Expression::EventValue(EventValueExpr {
                type_name: None,
                span,
            })
        }
        Token::EventValue(s) => {
            parser.advance();
            Expression::EventValue(EventValueExpr {
                type_name: Some(s),
                span,
            })
        }
        Token::Variable(parts) => {
            parser.advance();
            Expression::Variable(Variable {
                name: parts.join("::"),
                span,
            })
        }
        Token::Identifier(s) => {
            parser.advance();
            if s == "event" {
                Expression::EventValue(EventValueExpr {
                    type_name: None,
                    span,
                })
            } else {
                Expression::Variable(Variable { name: s, span })
            }
        }
        Token::Minus => {
            parser.advance();
            let operand = parse_precedence(parser, 10);
            let operand_span = span_of(&operand);
            Expression::Unary(UnaryExpr {
                operator: UnaryOp::Neg,
                operand: Box::new(operand),
                span: Span::new(span.start, operand_span.end),
            })
        }
        Token::Keyword("not") => {
            parser.advance();
            let operand = parse_precedence(parser, 10);
            let operand_span = span_of(&operand);
            Expression::Unary(UnaryExpr {
                operator: UnaryOp::Not,
                operand: Box::new(operand),
                span: Span::new(span.start, operand_span.end),
            })
        }
        Token::LParen => {
            parser.advance();
            parse_parenthesized_or_list(parser)
        }
        Token::StrText(_)
        | Token::StrInterpolationStart
        | Token::MiniMessageOpen(_)
        | Token::MiniMessageClose(_) => parse_interpolation(parser),
        Token::Keyword(s) => {
            // Unknown keywords in expression context are treated as
            // identifiers. Skript's natural-language syntax uses many
            // keywords as part of expressions: `at`, `of`, `within`,
            // `does`, `then`, etc. Treating them as identifiers lets the
            // effect-fallback parser handle the whole line rather than
            // erroring mid-expression.
            parser.advance();
            Expression::Variable(Variable {
                name: s.to_string(),
                span,
            })
        }
        _ => {
            parser.error(
                format!("Unexpected token {:?} in expression", tok.token),
                span,
            );
            parser.advance();
            Expression::Unknown(UnknownExpr {
                content: format!("{:?}", tok.token),
                span,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Core Pratt loop
// ---------------------------------------------------------------------------

/// Parse an expression with a minimum precedence bound.
///
/// This is the core of the Pratt parser. It first parses a prefix expression,
/// then loops to handle postfix (`.`, `(`, `[`) and infix (binary) operators.
fn parse_precedence(parser: &mut Parser, min_prec: u8) -> Expression {
    let mut left = parse_prefix(parser);

    loop {
        let token = parser.peek_token();

        // Postfix: property access via dot
        if token == Token::Dot {
            parser.advance();
            let prop_tok = parser.peek();
            let property = expect_identifier_or_keyword(parser);
            let prop_span = Span::new(span_of(&left).start, prop_tok.span.end);
            left = Expression::PropertyAccess(PropertyAccessExpr {
                object: Box::new(left),
                property,
                span: prop_span,
            });
            continue;
        }

        // Postfix: function call
        if token == Token::LParen {
            left = parse_function_call(parser, left);
            continue;
        }

        // Postfix: index
        if token == Token::LBracket {
            left = parse_index(parser, left);
            continue;
        }

        // Binary (infix) operators
        if let Some((prec, left_assoc)) = get_precedence(&token) {
            if prec < min_prec {
                break;
            }
            parser.advance();
            let right = parse_precedence(parser, if left_assoc { prec + 1 } else { prec });
            let operator = token_to_binary_op(&token);
            let bin_span = combine_spans(&left, &right);
            left = Expression::Binary(BinaryExpr {
                left: Box::new(left),
                operator,
                right: Box::new(right),
                span: bin_span,
            });
            continue;
        }

        break;
    }

    left
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn tokenize(source: &str) -> Vec<crate::lexer::SpannedToken> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let st = lexer.next_token();
            let is_eof = matches!(st.token, Token::Eof);
            tokens.push(st);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn parse_expr(source: &str) -> Expression {
        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        parse_expression(&mut parser)
    }

    // -----------------------------------------------------------------------
    // Assertion helpers
    // -----------------------------------------------------------------------

    fn assert_number(expr: &Expression, expected: &str) {
        match expr {
            Expression::Literal(Literal::Number(n)) => {
                assert_eq!(n.value, expected, "number value mismatch")
            }
            other => panic!("expected Number({}), got {:?}", expected, other),
        }
    }

    fn assert_string(expr: &Expression, expected: &str) {
        match expr {
            Expression::Literal(Literal::String(s)) => {
                assert_eq!(s.value, expected, "string value mismatch")
            }
            other => panic!("expected String({}), got {:?}", expected, other),
        }
    }

    fn assert_bool(expr: &Expression, expected: bool) {
        match expr {
            Expression::Literal(Literal::Boolean(b)) => {
                assert_eq!(b.value, expected, "bool value mismatch")
            }
            other => panic!("expected Boolean({}), got {:?}", expected, other),
        }
    }

    fn assert_variable(expr: &Expression, expected: &str) {
        match expr {
            Expression::Variable(v) => assert_eq!(v.name, expected, "variable name mismatch"),
            other => panic!("expected Variable({}), got {:?}", expected, other),
        }
    }

    fn assert_event_value(expr: &Expression, expected_type: Option<&str>) {
        match expr {
            Expression::EventValue(e) => {
                assert_eq!(
                    e.type_name.as_deref(),
                    expected_type,
                    "event-value type mismatch"
                )
            }
            other => panic!("expected EventValue({:?}), got {:?}", expected_type, other),
        }
    }

    fn assert_binary(expr: &Expression, op: BinaryOp) -> (&Expression, &Expression) {
        match expr {
            Expression::Binary(b) => {
                assert_eq!(b.operator, op, "binary operator mismatch");
                (&b.left, &b.right)
            }
            other => panic!("expected Binary({:?}), got {:?}", op, other),
        }
    }

    fn assert_unary(expr: &Expression, op: UnaryOp) -> &Expression {
        match expr {
            Expression::Unary(u) => {
                assert_eq!(u.operator, op, "unary operator mismatch");
                &u.operand
            }
            other => panic!("expected Unary({:?}), got {:?}", op, other),
        }
    }

    fn assert_function_call(expr: &Expression) -> (&Expression, &[Expression]) {
        match expr {
            Expression::FunctionCall(f) => (&f.callee, &f.arguments),
            other => panic!("expected FunctionCall, got {:?}", other),
        }
    }

    fn assert_index(expr: &Expression) -> (&Expression, &Expression) {
        match expr {
            Expression::Index(i) => (&i.target, &i.index),
            other => panic!("expected Index, got {:?}", other),
        }
    }

    fn assert_property_access(expr: &Expression) -> (&Expression, &str) {
        match expr {
            Expression::PropertyAccess(p) => (&p.object, p.property.as_str()),
            other => panic!("expected PropertyAccess, got {:?}", other),
        }
    }

    fn assert_parenthesized(expr: &Expression) -> &Expression {
        match expr {
            Expression::Parenthesized(p) => &p.expression,
            other => panic!("expected Parenthesized, got {:?}", other),
        }
    }

    fn assert_list(expr: &Expression) -> usize {
        match expr {
            Expression::List(l) => l.elements.len(),
            other => panic!("expected List, got {:?}", other),
        }
    }

    fn assert_unknown(expr: &Expression) -> &str {
        match expr {
            Expression::Unknown(u) => u.content.as_str(),
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_number() {
        let expr = parse_expr("42");
        assert_number(&expr, "42");
    }

    #[test]
    fn test_parse_number_float() {
        let expr = parse_expr("3.14");
        assert_number(&expr, "3.14");
    }

    #[test]
    fn test_parse_string() {
        let expr = parse_expr(r#""hello""#);
        assert_string(&expr, "hello");
    }

    #[test]
    fn test_parse_boolean_true() {
        let expr = parse_expr("true");
        assert_bool(&expr, true);
    }

    #[test]
    fn test_parse_boolean_false() {
        let expr = parse_expr("false");
        assert_bool(&expr, false);
    }

    // -----------------------------------------------------------------------
    // Variables
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_variable_simple() {
        let expr = parse_expr("{_var}");
        assert_variable(&expr, "_var");
    }

    #[test]
    fn test_parse_variable_list() {
        let expr = parse_expr("{list::*}");
        assert_variable(&expr, "list::*");
    }

    #[test]
    fn test_parse_identifier_as_variable() {
        let expr = parse_expr("player");
        assert_variable(&expr, "player");
    }

    // -----------------------------------------------------------------------
    // Event values
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_event_keyword() {
        let expr = parse_expr("event");
        assert_event_value(&expr, None);
    }

    // -----------------------------------------------------------------------
    // Unary operators
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_unary_neg() {
        let expr = parse_expr("- 5");
        let operand = assert_unary(&expr, UnaryOp::Neg);
        assert_number(operand, "5");
    }

    #[test]
    fn test_parse_unary_not() {
        let expr = parse_expr("not true");
        let operand = assert_unary(&expr, UnaryOp::Not);
        assert_bool(operand, true);
    }

    #[test]
    fn test_parse_double_neg() {
        let expr = parse_expr("- - 5");
        let inner = assert_unary(&expr, UnaryOp::Neg);
        let operand = assert_unary(inner, UnaryOp::Neg);
        assert_number(operand, "5");
    }

    // -----------------------------------------------------------------------
    // Binary arithmetic
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_binary_add() {
        let expr = parse_expr("1 + 2");
        let (left, right) = assert_binary(&expr, BinaryOp::Add);
        assert_number(left, "1");
        assert_number(right, "2");
    }

    #[test]
    fn test_parse_binary_sub() {
        let expr = parse_expr("3 - 1");
        let (left, right) = assert_binary(&expr, BinaryOp::Sub);
        assert_number(left, "3");
        assert_number(right, "1");
    }

    #[test]
    fn test_parse_binary_mul() {
        let expr = parse_expr("2 * 3");
        let (left, right) = assert_binary(&expr, BinaryOp::Mul);
        assert_number(left, "2");
        assert_number(right, "3");
    }

    #[test]
    fn test_parse_binary_div() {
        let expr = parse_expr("6 / 2");
        let (left, right) = assert_binary(&expr, BinaryOp::Div);
        assert_number(left, "6");
        assert_number(right, "2");
    }

    #[test]
    fn test_parse_binary_mod() {
        let expr = parse_expr("5 % 2");
        let (left, right) = assert_binary(&expr, BinaryOp::Mod);
        assert_number(left, "5");
        assert_number(right, "2");
    }

    #[test]
    fn test_parse_binary_pow() {
        let expr = parse_expr("2 ^ 3");
        let (left, right) = assert_binary(&expr, BinaryOp::Pow);
        assert_number(left, "2");
        assert_number(right, "3");
    }

    #[test]
    fn test_parse_binary_concat() {
        let expr = parse_expr(r#""a" ~ "b""#);
        let (left, right) = assert_binary(&expr, BinaryOp::Concat);
        assert_string(left, "a");
        assert_string(right, "b");
    }

    // -----------------------------------------------------------------------
    // Comparison and equality
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_less_than() {
        let expr = parse_expr("a < b");
        let (left, right) = assert_binary(&expr, BinaryOp::Lt);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_greater_than() {
        let expr = parse_expr("a > b");
        let (left, right) = assert_binary(&expr, BinaryOp::Gt);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_less_equal() {
        let expr = parse_expr("a <= b");
        let (left, right) = assert_binary(&expr, BinaryOp::Leq);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_greater_equal() {
        let expr = parse_expr("a >= b");
        let (left, right) = assert_binary(&expr, BinaryOp::Geq);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_equal_equal() {
        let expr = parse_expr("a == b");
        let (left, right) = assert_binary(&expr, BinaryOp::Eq);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_not_equal() {
        let expr = parse_expr("a != b");
        let (left, right) = assert_binary(&expr, BinaryOp::Neq);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_is() {
        let expr = parse_expr("a is b");
        let (left, right) = assert_binary(&expr, BinaryOp::Eq);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_isnt() {
        let expr = parse_expr("a isn't b");
        let (left, right) = assert_binary(&expr, BinaryOp::Neq);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    // -----------------------------------------------------------------------
    // Logical operators
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_and() {
        let expr = parse_expr("a and b");
        let (left, right) = assert_binary(&expr, BinaryOp::And);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_or() {
        let expr = parse_expr("a or b");
        let (left, right) = assert_binary(&expr, BinaryOp::Or);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_in() {
        let expr = parse_expr("a in b");
        let (left, right) = assert_binary(&expr, BinaryOp::In);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    #[test]
    fn test_parse_instanceof() {
        let expr = parse_expr("a instanceof b");
        let (left, right) = assert_binary(&expr, BinaryOp::InstanceOf);
        assert_variable(left, "a");
        assert_variable(right, "b");
    }

    // -----------------------------------------------------------------------
    // Precedence
    // -----------------------------------------------------------------------

    #[test]
    fn test_precedence_mul_over_add() {
        // 1 + 2 * 3  → 1 + (2 * 3)
        let expr = parse_expr("1 + 2 * 3");
        let (left, right) = assert_binary(&expr, BinaryOp::Add);
        assert_number(left, "1");
        let (r_left, r_right) = assert_binary(right, BinaryOp::Mul);
        assert_number(r_left, "2");
        assert_number(r_right, "3");
    }

    #[test]
    fn test_precedence_add_over_mul() {
        // (1 + 2) * 3
        let expr = parse_expr("(1 + 2) * 3");
        let (left, right) = assert_binary(&expr, BinaryOp::Mul);
        assert_number(right, "3");
        let inner = assert_parenthesized(left);
        let (a, b) = assert_binary(inner, BinaryOp::Add);
        assert_number(a, "1");
        assert_number(b, "2");
    }

    #[test]
    fn test_precedence_and_over_or() {
        // a or b and c  → a or (b and c)
        let expr = parse_expr("a or b and c");
        let (left, right) = assert_binary(&expr, BinaryOp::Or);
        assert_variable(left, "a");
        let (r_left, r_right) = assert_binary(right, BinaryOp::And);
        assert_variable(r_left, "b");
        assert_variable(r_right, "c");
    }

    #[test]
    fn test_precedence_comparison_over_logical() {
        // a and b < c  → a and (b < c)
        let expr = parse_expr("a and b < c");
        let (left, right) = assert_binary(&expr, BinaryOp::And);
        assert_variable(left, "a");
        let (r_left, r_right) = assert_binary(right, BinaryOp::Lt);
        assert_variable(r_left, "b");
        assert_variable(r_right, "c");
    }

    #[test]
    fn test_precedence_concat_over_comparison() {
        // a < b ~ c  → a < (b ~ c)
        let expr = parse_expr("a < b ~ c");
        let (left, right) = assert_binary(&expr, BinaryOp::Lt);
        assert_variable(left, "a");
        let (r_left, r_right) = assert_binary(right, BinaryOp::Concat);
        assert_variable(r_left, "b");
        assert_variable(r_right, "c");
    }

    #[test]
    fn test_precedence_arithmetic_over_concat() {
        // a ~ b + c  → a ~ (b + c)
        let expr = parse_expr("a ~ b + c");
        let (left, right) = assert_binary(&expr, BinaryOp::Concat);
        assert_variable(left, "a");
        let (r_left, r_right) = assert_binary(right, BinaryOp::Add);
        assert_variable(r_left, "b");
        assert_variable(r_right, "c");
    }

    #[test]
    fn test_precedence_mul_over_sub() {
        // 5 - 2 * 3  → 5 - (2 * 3)
        let expr = parse_expr("5 - 2 * 3");
        let (left, right) = assert_binary(&expr, BinaryOp::Sub);
        assert_number(left, "5");
        let (r_left, r_right) = assert_binary(right, BinaryOp::Mul);
        assert_number(r_left, "2");
        assert_number(r_right, "3");
    }

    #[test]
    fn test_power_right_associativity() {
        // 2 ^ 3 ^ 4  → 2 ^ (3 ^ 4)
        let expr = parse_expr("2 ^ 3 ^ 4");
        let (left, right) = assert_binary(&expr, BinaryOp::Pow);
        assert_number(left, "2");
        let (r_left, r_right) = assert_binary(right, BinaryOp::Pow);
        assert_number(r_left, "3");
        assert_number(r_right, "4");
    }

    // -----------------------------------------------------------------------
    // Parenthesized and list expressions
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_parenthesized() {
        let expr = parse_expr("(42)");
        let inner = assert_parenthesized(&expr);
        assert_number(inner, "42");
    }

    #[test]
    fn test_parse_nested_parenthesized() {
        let expr = parse_expr("((1 + 2))");
        let inner1 = assert_parenthesized(&expr);
        let inner2 = assert_parenthesized(inner1);
        let (l, r) = assert_binary(inner2, BinaryOp::Add);
        assert_number(l, "1");
        assert_number(r, "2");
    }

    #[test]
    fn test_parse_empty_list() {
        let expr = parse_expr("()");
        assert_eq!(assert_list(&expr), 0);
    }

    #[test]
    fn test_parse_list_one_element() {
        // A single-element list: (1,)
        let expr = parse_expr("(1,)");
        assert_eq!(assert_list(&expr), 1);
    }

    #[test]
    fn test_parse_list_two_elements() {
        let expr = parse_expr("(1, 2)");
        assert_eq!(assert_list(&expr), 2);
    }

    #[test]
    fn test_parse_list_three_elements() {
        let expr = parse_expr("(1, 2, 3)");
        assert_eq!(assert_list(&expr), 3);
    }

    // -----------------------------------------------------------------------
    // Function calls
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_function_call_no_args() {
        let expr = parse_expr("foo()");
        let (callee, args) = assert_function_call(&expr);
        assert_variable(callee, "foo");
        assert!(args.is_empty(), "expected no arguments");
    }

    #[test]
    fn test_parse_function_call_one_arg() {
        let expr = parse_expr("foo(1)");
        let (callee, args) = assert_function_call(&expr);
        assert_variable(callee, "foo");
        assert_eq!(args.len(), 1);
        assert_number(&args[0], "1");
    }

    #[test]
    fn test_parse_function_call_two_args() {
        let expr = parse_expr("foo(1, 2)");
        let (callee, args) = assert_function_call(&expr);
        assert_variable(callee, "foo");
        assert_eq!(args.len(), 2);
        assert_number(&args[0], "1");
        assert_number(&args[1], "2");
    }

    #[test]
    fn test_parse_function_call_nested_arg() {
        let expr = parse_expr("foo(1 + 2)");
        let (callee, args) = assert_function_call(&expr);
        assert_variable(callee, "foo");
        assert_eq!(args.len(), 1);
        let (l, r) = assert_binary(&args[0], BinaryOp::Add);
        assert_number(l, "1");
        assert_number(r, "2");
    }

    #[test]
    fn test_parse_chained_function_calls() {
        let expr = parse_expr("foo()()");
        let (outer_callee, outer_args) = assert_function_call(&expr);
        assert!(outer_args.is_empty());
        let (inner_callee, inner_args) = assert_function_call(outer_callee);
        assert_variable(inner_callee, "foo");
        assert!(inner_args.is_empty());
    }

    // -----------------------------------------------------------------------
    // Index
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_index() {
        let expr = parse_expr("{_arr}[0]");
        let (target, index) = assert_index(&expr);
        assert_variable(target, "_arr");
        assert_number(index, "0");
    }

    #[test]
    fn test_parse_nested_index() {
        let expr = parse_expr("a[b[c]]");
        let (target1, index1) = assert_index(&expr);
        assert_variable(target1, "a");
        let (target2, index2) = assert_index(index1);
        assert_variable(target2, "b");
        assert_variable(index2, "c");
    }

    // -----------------------------------------------------------------------
    // Property access
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_property_access() {
        let expr = parse_expr("obj.prop");
        let (object, property) = assert_property_access(&expr);
        assert_variable(object, "obj");
        assert_eq!(property, "prop");
    }

    #[test]
    fn test_parse_chained_property_access() {
        let expr = parse_expr("a.b.c");
        let (outer_obj, outer_prop) = assert_property_access(&expr);
        assert_eq!(outer_prop, "c");
        let (inner_obj, inner_prop) = assert_property_access(outer_obj);
        assert_variable(inner_obj, "a");
        assert_eq!(inner_prop, "b");
    }

    #[test]
    fn test_parse_method_call() {
        let expr = parse_expr("obj.method()");
        let (callee, args) = assert_function_call(&expr);
        assert!(args.is_empty());
        let (object, property) = assert_property_access(callee);
        assert_variable(object, "obj");
        assert_eq!(property, "method");
    }

    // -----------------------------------------------------------------------
    // Mixed expressions
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_complex_expression() {
        // (a + b) * c[d] - e.f(g)
        // Should parse as: Sub(Mul(Parens(Add(a, b)), Index(c, d)), Call(Prop(e, f), [g]))
        let expr = parse_expr("(a + b) * c[d] - e.f(g)");
        let (left, right) = assert_binary(&expr, BinaryOp::Sub);

        // left: (a + b) * c[d]
        let (l_left, l_right) = assert_binary(left, BinaryOp::Mul);
        let add = assert_parenthesized(l_left);
        let (add_l, add_r) = assert_binary(add, BinaryOp::Add);
        assert_variable(add_l, "a");
        assert_variable(add_r, "b");
        let (index_target, index_idx) = assert_index(l_right);
        assert_variable(index_target, "c");
        assert_variable(index_idx, "d");

        // right: e.f(g)
        let (call_callee, call_args) = assert_function_call(right);
        assert_eq!(call_args.len(), 1);
        assert_variable(&call_args[0], "g");
        let (prop_obj, prop_name) = assert_property_access(call_callee);
        assert_variable(prop_obj, "e");
        assert_eq!(prop_name, "f");
    }

    // -----------------------------------------------------------------------
    // Error recovery
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_recovery_invalid_token() {
        let tokens = tokenize("|");
        let mut parser = Parser::new(tokens);
        let expr = parse_expression(&mut parser);
        assert!(
            !parser.errors.is_empty(),
            "expected errors for invalid token"
        );
        assert_unknown(&expr);
    }

    #[test]
    fn test_error_recovery_partial_expression() {
        // After an error, the parser should recover and not panic
        let tokens = tokenize("| 42");
        let mut parser = Parser::new(tokens);
        let expr = parse_expression(&mut parser);
        assert!(!parser.errors.is_empty(), "expected errors");
        // Second token should not be parsed as part of the expression
        // (the unknown token consumes the Pipe, then the loop finds
        // no infix operator and returns the Unknown expression)
        assert_unknown(&expr);
    }

    #[test]
    fn test_error_recovery_missing_rparen() {
        let tokens = tokenize("(1 + 2");
        let mut parser = Parser::new(tokens);
        let expr = parse_expression(&mut parser);
        // Should produce a parenthesized expression with an error for missing RParen
        assert!(
            !parser.errors.is_empty(),
            "expected errors for missing RParen"
        );
        // But still parse the inner expression
        match expr {
            Expression::Parenthesized(p) => {
                let (l, r) = assert_binary(&p.expression, BinaryOp::Add);
                assert_number(l, "1");
                assert_number(r, "2");
            }
            other => panic!("expected Parenthesized, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Span sanity checks
    // -----------------------------------------------------------------------

    #[test]
    fn test_spans_are_set() {
        let expr = parse_expr("42");
        let span = span_of(&expr);
        assert!(span.start < span.end, "span should be non-empty");
    }

    #[test]
    fn test_binary_span_covers_whole_expression() {
        let expr = parse_expr("1 + 2");
        let span = span_of(&expr);
        assert_eq!(span.start, 0, "binary expression should start at 0");
    }
}
