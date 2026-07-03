use crate::ast::*;
use crate::lexer::Token;
use super::Parser;
use super::expr::parse_expression;

// ---------------------------------------------------------------------------
// Span helper
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

fn token_text(token: &Token) -> String {
    match token {
        Token::Identifier(s) | Token::Number(s) | Token::String(s) | Token::StrText(s) => {
            s.clone()
        }
        Token::Keyword(s) => s.to_string(),
        Token::Boolean(v) => v.to_string(),
        Token::Variable(parts) => format!("{{{}}}", parts.join("::")),
        _ => format!("{:?}", token),
    }
}

// ---------------------------------------------------------------------------
// parse_statement — top-level statement dispatcher
// ---------------------------------------------------------------------------

/// Parse a single statement from the current position.
pub fn parse_statement(parser: &mut Parser) -> Statement {
    while parser.consume_if(&Token::Newline) {}

    if matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
        return Statement::BlankLine;
    }

    let token = parser.peek_token();
    match token {
        Token::Keyword("if") => parse_if(parser),
        Token::Keyword("loop") => parse_loop(parser),
        Token::Keyword("while") => parse_while(parser),
        Token::Keyword("for") => parse_for_in(parser),
        Token::Keyword("set") => parse_assignment(parser),
        Token::Keyword("add") | Token::Keyword("remove") => parse_add_remove_effect(parser),
        Token::Keyword("send") | Token::Keyword("broadcast") => parse_send_effect(parser),
        Token::Keyword("delete")
        | Token::Keyword("clear")
        | Token::Keyword("give")
        | Token::Keyword("teleport") => parse_effect(parser),
        Token::Keyword("return") => parse_return(parser),
        Token::Keyword("stop") => {
            let tok = parser.advance();
            Statement::Stop(StopStmt { span: tok.span })
        }
        Token::Keyword("exit") => {
            let tok = parser.advance();
            let _ = parser.consume_if(&Token::Keyword("loop"));
            Statement::Exit(ExitStmt { span: tok.span })
        }
        Token::Keyword("continue") => {
            let tok = parser.advance();
            Statement::Continue(ContinueStmt { span: tok.span })
        }
        Token::Keyword("cancel") => parse_cancel_event(parser),
        Token::Keyword("wait") => parse_wait(parser),
        Token::Keyword("do") => parse_do_while(parser),
        _ => parse_effect_statement(parser),
    }
}

// ---------------------------------------------------------------------------
// Block body
// ---------------------------------------------------------------------------

/// Parse an indented block of statements.
///
/// Expects an optional `Newline` followed by `Indent`, then statements until
/// `Dedent` or `Eof`. Returns an empty vector when no indented block follows.
pub fn parse_block_body(parser: &mut Parser) -> Vec<Statement> {
    if matches!(parser.peek_token(), Token::Newline) {
        parser.advance();
    }

    if !matches!(parser.peek_token(), Token::Indent) {
        return Vec::new();
    }

    parser.advance();

    let mut stmts = Vec::new();
    loop {
        while parser.consume_if(&Token::Newline) {}

        if matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
            break;
        }

        stmts.push(parse_statement(parser));
    }

    parser.expect(&Token::Dedent);
    stmts
}

// ---------------------------------------------------------------------------
// If / else if / else
// ---------------------------------------------------------------------------

fn parse_if(parser: &mut Parser) -> Statement {
    let start = parser.advance().span;

    let condition = parse_expression(parser);

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    let (else_if_branches, else_branch) = parse_else_chain(parser);

    let end = if let Some(ref el) = else_branch {
        el.span.end
    } else if let Some(last) = else_if_branches.last() {
        last.span.end
    } else {
        parser.peek().span.start
    };

    Statement::If(IfStmt {
        condition,
        body,
        else_if: else_if_branches,
        else_branch,
        span: Span::new(start.start, end),
    })
}

fn parse_else_chain(
    parser: &mut Parser,
) -> (Vec<ElseIfStmt>, Option<Box<ElseStmt>>) {
    let mut else_if_branches = Vec::new();
    let mut else_branch = None;

    loop {
        while parser.consume_if(&Token::Newline) {}

        if !matches!(parser.peek_token(), Token::Keyword("else")) {
            break;
        }

        let else_span = parser.peek().span;
        parser.advance();

        if parser.consume_if(&Token::Keyword("if")) {
            let condition = parse_expression(parser);
            parser.expect(&Token::Colon);
            let elif_body = parse_block_body(parser);
            let elif_end = parser.peek().span.start;

            else_if_branches.push(ElseIfStmt {
                condition,
                body: elif_body,
                span: Span::new(else_span.start, elif_end),
            });
            continue;
        }

        parser.expect(&Token::Colon);
        let else_body = parse_block_body(parser);
        let else_end = parser.peek().span.start;

        else_branch = Some(Box::new(ElseStmt {
            body: else_body,
            span: Span::new(else_span.start, else_end),
        }));
        break;
    }

    (else_if_branches, else_branch)
}

// ---------------------------------------------------------------------------
// Loop
// ---------------------------------------------------------------------------

fn parse_loop(parser: &mut Parser) -> Statement {
    let start = parser.advance().span;

    if !matches!(parser.peek_token(), Token::Colon) {
        parse_expression(parser);
        if matches!(parser.peek_token(), Token::Identifier(ref s) if s == "times") {
            parser.advance();
        }
    }

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    Statement::Loop(LoopStmt {
        body,
        span: Span::new(start.start, parser.peek().span.start),
    })
}

// ---------------------------------------------------------------------------
// While
// ---------------------------------------------------------------------------

fn parse_while(parser: &mut Parser) -> Statement {
    let start = parser.advance().span;

    let condition = parse_expression(parser);

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    Statement::While(WhileStmt {
        condition,
        body,
        span: Span::new(start.start, parser.peek().span.start),
    })
}

// ---------------------------------------------------------------------------
// Do-While
// ---------------------------------------------------------------------------

fn parse_do_while(parser: &mut Parser) -> Statement {
    let do_token = parser.advance();
    let start = do_token.span;

    // "do:" — bare do block (execute body once)
    if matches!(parser.peek_token(), Token::Colon) {
        parser.advance();
        let body = parse_block_body(parser);
        let end = parser.peek().span.start;
        return Statement::DoWhile(DoWhileStmt {
            condition: Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: true,
                span: Span::new(start.start, end),
            })),
            body,
            span: Span::new(start.start, end),
        });
    }

    // "do <effect>" — fall back to effect
    if !matches!(parser.peek_token(), Token::Keyword("while")) {
        let effect = "do".to_string();
        let mut args = Vec::new();
        while !matches!(
            parser.peek_token(),
            Token::Newline | Token::Dedent | Token::Eof | Token::Colon
        ) {
            args.push(parse_expression(parser));
        }
        let end = args.last().map(|e| span_of(e).end).unwrap_or(start.end);
        return Statement::Effect(EffectStmt {
            effect,
            arguments: args,
            span: Span::new(start.start, end),
        });
    }

    parser.advance(); // consume "while"

    // "do while:" — no condition provided
    if matches!(parser.peek_token(), Token::Colon) {
        parser.advance();
        let body = parse_block_body(parser);
        let end = parser.peek().span.start;
        return Statement::DoWhile(DoWhileStmt {
            condition: Expression::Literal(Literal::Boolean(BooleanLiteral {
                value: true,
                span: Span::new(start.start, end),
            })),
            body,
            span: Span::new(start.start, end),
        });
    }

    let condition = parse_expression(parser);
    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    Statement::DoWhile(DoWhileStmt {
        body,
        condition,
        span: Span::new(start.start, parser.peek().span.start),
    })
}

// ---------------------------------------------------------------------------
// For-In
// ---------------------------------------------------------------------------

fn parse_for_in(parser: &mut Parser) -> Statement {
    let start = parser.advance().span;

    if matches!(parser.peek_token(), Token::Keyword("each"))
        || matches!(parser.peek_token(), Token::Identifier(ref s) if s == "each")
    {
        parser.advance();
    }

    let variable = match parser.peek_token() {
        Token::Identifier(ref s) => {
            let name = s.clone();
            parser.advance();
            name
        }
        Token::Variable(ref parts) => {
            let name = parts.join("::");
            parser.advance();
            name
        }
        Token::Keyword(ref kw) => {
            let name = kw.to_string();
            parser.advance();
            name
        }
        _ => {
            let tok = parser.peek();
            parser.error("Expected loop variable", tok.span);
            String::new()
        }
    };

    if !parser.consume_if(&Token::Keyword("in")) {
        parser.error("Expected 'in' in for loop", parser.peek().span);
    }

    let iterable = parse_expression(parser);

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    Statement::ForIn(ForInStmt {
        variable,
        iterable,
        body,
        span: Span::new(start.start, parser.peek().span.start),
    })
}

// ---------------------------------------------------------------------------
// Assignment
// ---------------------------------------------------------------------------

fn parse_assignment(parser: &mut Parser) -> Statement {
    let start = parser.advance().span;

    let target = parse_assignment_target(parser);

    let operator = if parser.consume_if(&Token::Keyword("to")) {
        AssignmentOp::Equal
    } else if parser.consume_if(&Token::Equal) {
        AssignmentOp::Equal
    } else if parser.consume_if(&Token::ColonEqual) {
        AssignmentOp::ColonEqual
    } else if parser.consume_if(&Token::PlusEqual) {
        AssignmentOp::PlusEqual
    } else if parser.consume_if(&Token::MinusEqual) {
        AssignmentOp::MinusEqual
    } else if parser.consume_if(&Token::StarEqual) {
        AssignmentOp::StarEqual
    } else if parser.consume_if(&Token::SlashEqual) {
        AssignmentOp::SlashEqual
    } else {
        parser.error(
            "Expected assignment operator (=, :=, +=, -=, *=, /=, or 'to')",
            parser.peek().span,
        );
        AssignmentOp::Equal
    };

    let value = parse_expression(parser);
    let end = span_of(&value);

    Statement::Assignment(AssignmentStmt {
        target,
        value,
        operator,
        span: Span::new(start.start, end.end),
    })
}

fn parse_assignment_target(parser: &mut Parser) -> AssignmentTarget {
    let tok = parser.peek();
    match &tok.token {
        Token::Variable(_) => {
            let expr = parse_expression(parser);
            match expr {
                Expression::Variable(v) => AssignmentTarget::Variable(v),
                _ => {
                    let s = span_of(&expr);
                    parser.error("Expected variable as assignment target", s);
                    AssignmentTarget::Variable(Variable {
                        name: String::new(),
                        span: s,
                    })
                }
            }
        }
        Token::Identifier(_) | Token::Keyword(_) => {
            let expr = parse_expression(parser);
            target_from_expression(parser, expr)
        }
        _ => {
            parser.error("Expected assignment target", tok.span);
            parser.advance();
            AssignmentTarget::Variable(Variable {
                name: String::new(),
                span: tok.span,
            })
        }
    }
}

fn target_from_expression(parser: &mut Parser, expr: Expression) -> AssignmentTarget {
    match expr {
        Expression::Variable(v) => AssignmentTarget::Variable(v),
        Expression::PropertyAccess(p) => AssignmentTarget::PropertyAccess(p),
        Expression::Index(i) => AssignmentTarget::Index(i),
        _ => {
            let s = span_of(&expr);
            parser.error(
                "Expected variable, property access, or index as assignment target",
                s,
            );
            AssignmentTarget::Variable(Variable {
                name: String::new(),
                span: s,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Effect statements
// ---------------------------------------------------------------------------

fn parse_effect(parser: &mut Parser) -> Statement {
    let tok = parser.advance();
    let effect = token_text(&tok.token);
    let start = tok.span;

    let mut args = Vec::new();
    while !matches!(
        parser.peek_token(),
        Token::Newline | Token::Dedent | Token::Eof | Token::Colon
    ) {
        args.push(parse_expression(parser));
    }

    let end = args.last().map(|e| span_of(e).end).unwrap_or(start.end);

    Statement::Effect(EffectStmt {
        effect,
        arguments: args,
        span: Span::new(start.start, end),
    })
}

fn parse_add_remove_effect(parser: &mut Parser) -> Statement {
    let tok = parser.advance();
    let effect = token_text(&tok.token);
    let start = tok.span;

    let value = parse_expression(parser);

    if effect == "add" {
        if !parser.consume_if(&Token::Keyword("to")) {
            parser.error("Expected 'to' after add value", parser.peek().span);
        }
    } else {
        if !parser.consume_if(&Token::Keyword("from")) {
            parser.error("Expected 'from' after remove value", parser.peek().span);
        }
    }

    let target = parse_expression(parser);
    let end = span_of(&target);

    Statement::Effect(EffectStmt {
        effect,
        arguments: vec![value, target],
        span: Span::new(start.start, end.end),
    })
}

fn parse_send_effect(parser: &mut Parser) -> Statement {
    let tok = parser.advance();
    let effect = token_text(&tok.token);
    let start = tok.span;

    let message = parse_expression(parser);

    let mut args = vec![message];
    if parser.consume_if(&Token::Keyword("to")) {
        let target = parse_expression(parser);
        args.push(target);
    }

    let end = args.last().map(|e| span_of(e).end).unwrap_or(start.end);

    Statement::Effect(EffectStmt {
        effect,
        arguments: args,
        span: Span::new(start.start, end),
    })
}

fn parse_effect_statement(parser: &mut Parser) -> Statement {
    let tok = parser.peek();
    let span = tok.span;

    match &tok.token {
        Token::Identifier(_) | Token::Keyword(_) => {
            let effect = token_text(&tok.token);
            parser.advance();
            let mut args = Vec::new();
            while !matches!(
                parser.peek_token(),
                Token::Newline | Token::Dedent | Token::Eof | Token::Colon
            ) {
                args.push(parse_expression(parser));
            }
            let end = args.last().map(|e| span_of(e).end).unwrap_or(span.end);
            Statement::Effect(EffectStmt {
                effect,
                arguments: args,
                span: Span::new(span.start, end),
            })
        }
        _ => {
            parser.error("Expected statement", span);
            parser.advance();
            parser.sync();
            Statement::BlankLine
        }
    }
}

// ---------------------------------------------------------------------------
// Return
// ---------------------------------------------------------------------------

fn parse_return(parser: &mut Parser) -> Statement {
    let tok = parser.advance();
    let start = tok.span;

    let value = if !matches!(
        parser.peek_token(),
        Token::Newline | Token::Dedent | Token::Eof
    ) {
        Some(parse_expression(parser))
    } else {
        None
    };

    let end = value.as_ref().map(|e| span_of(e).end).unwrap_or(start.end);

    Statement::Return(ReturnStmt {
        value,
        span: Span::new(start.start, end),
    })
}

// ---------------------------------------------------------------------------
// Cancel event
// ---------------------------------------------------------------------------

fn parse_cancel_event(parser: &mut Parser) -> Statement {
    let tok = parser.advance();
    let start = tok.span;

    let _ = parser.consume_if(&Token::Keyword("event"));

    Statement::CancelEvent(CancelEventStmt {
        span: Span::new(start.start, parser.peek().span.start),
    })
}

// ---------------------------------------------------------------------------
// Wait
// ---------------------------------------------------------------------------

fn parse_wait(parser: &mut Parser) -> Statement {
    let tok = parser.advance();
    let start = tok.span;

    let duration = if !matches!(
        parser.peek_token(),
        Token::Newline | Token::Dedent | Token::Eof
    ) {
        Some(parse_expression(parser))
    } else {
        None
    };

    let end = duration.as_ref().map(|e| span_of(e).end).unwrap_or(start.end);

    Statement::Wait(WaitStmt {
        duration,
        span: Span::new(start.start, end),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::lexer::SpannedToken;

    fn tokenize(source: &str) -> Vec<SpannedToken> {
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

    fn parse_stmt(source: &str) -> (Parser, Statement) {
        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let stmt = parse_statement(&mut parser);
        (parser, stmt)
    }

    // -----------------------------------------------------------------------
    // If
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_if_simple() {
        let source = "if true:\n    broadcast \"hi\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::If(IfStmt {
                condition,
                body,
                else_if,
                else_branch,
                ..
            }) => {
                assert_eq!(body.len(), 1);
                assert!(else_if.is_empty());
                assert!(else_branch.is_none());
                match condition {
                    Expression::Literal(Literal::Boolean(b)) => assert!(b.value),
                    _ => panic!("expected boolean condition"),
                }
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_else() {
        let source =
            "if true:\n    broadcast \"hi\"\nelse:\n    broadcast \"bye\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::If(IfStmt {
                body,
                else_if,
                else_branch,
                ..
            }) => {
                assert_eq!(body.len(), 1);
                assert!(else_if.is_empty());
                assert!(else_branch.is_some());
                let else_body = &else_branch.unwrap().body;
                assert_eq!(else_body.len(), 1);
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_else_if_else() {
        let source = "if true:\n    broadcast \"hi\"\nelse if false:\n    broadcast \"no\"\nelse:\n    broadcast \"bye\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::If(IfStmt {
                body,
                else_if,
                else_branch,
                ..
            }) => {
                assert_eq!(body.len(), 1);
                assert_eq!(else_if.len(), 1);
                assert!(else_branch.is_some());
                assert_eq!(else_branch.unwrap().body.len(), 1);
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_multiple_else_if() {
        let source = "if a is b:\n    broadcast \"1\"\nelse if a is c:\n    broadcast \"2\"\nelse if a is d:\n    broadcast \"3\"\nelse:\n    broadcast \"4\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::If(IfStmt {
                else_if, else_branch, ..
            }) => {
                assert_eq!(else_if.len(), 2);
                assert!(else_branch.is_some());
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // While
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_while() {
        let source = "while player is online:\n    broadcast \"hi\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::While(WhileStmt { condition, body, .. }) => {
                assert_eq!(body.len(), 1);
                match condition {
                    Expression::Binary(_) => {}
                    _ => panic!("expected binary expression"),
                }
            }
            other => panic!("expected While, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_while_empty_body() {
        let source = "while false:";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::While(WhileStmt { body, .. }) => {
                assert!(body.is_empty());
            }
            other => panic!("expected While, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Do-While
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_do_as_effect() {
        let source = "do something";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt { effect, arguments, .. }) => {
                assert_eq!(effect, "do");
                assert_eq!(arguments.len(), 1);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // For-In
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_for_in_identifier() {
        let source = "for each player in {_players}:\n    broadcast player";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::ForIn(ForInStmt {
                variable,
                iterable,
                body,
                ..
            }) => {
                assert_eq!(variable, "player");
                assert_eq!(body.len(), 1);
                match iterable {
                    Expression::Variable(v) => assert_eq!(v.name, "_players"),
                    _ => panic!("expected variable"),
                }
            }
            other => panic!("expected ForIn, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_for_in_variable() {
        let source = "for {_player} in {_players}:\n    broadcast {_player}";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::ForIn(ForInStmt {
                variable,
                iterable,
                body,
                ..
            }) => {
                assert_eq!(variable, "_player");
                assert_eq!(body.len(), 1);
                match iterable {
                    Expression::Variable(v) => assert_eq!(v.name, "_players"),
                    _ => panic!("expected variable"),
                }
            }
            other => panic!("expected ForIn, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_for_in_without_each() {
        let source = "for player in {_players}:\n    broadcast player";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::ForIn(ForInStmt {
                variable, body, ..
            }) => {
                assert_eq!(variable, "player");
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected ForIn, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Loop
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_loop_times() {
        let source = "loop 5 times:\n    broadcast \"hi\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Loop(LoopStmt { body, .. }) => {
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected Loop, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_loop_list() {
        let source = "loop {_players}:\n    broadcast loop-player";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Loop(LoopStmt { body, .. }) => {
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected Loop, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Assignment
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_assignment_simple() {
        let source = "set {_x} to 5";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Assignment(AssignmentStmt {
                target,
                value,
                operator,
                ..
            }) => {
                assert_eq!(operator, AssignmentOp::Equal);
                match target {
                    AssignmentTarget::Variable(v) => assert_eq!(v.name, "_x"),
                    _ => panic!("expected variable target"),
                }
                match value {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(n.value, "5"),
                    _ => panic!("expected number"),
                }
            }
            other => panic!("expected Assignment, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_assignment_identifier() {
        let source = "set x to 10";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Assignment(AssignmentStmt {
                target,
                value,
                operator,
                ..
            }) => {
                assert_eq!(operator, AssignmentOp::Equal);
                match target {
                    AssignmentTarget::Variable(v) => assert_eq!(v.name, "x"),
                    _ => panic!("expected variable target"),
                }
                match value {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(n.value, "10"),
                    _ => panic!("expected number"),
                }
            }
            other => panic!("expected Assignment, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_assignment_operator_equal() {
        let source = "set {_x} = 5";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Assignment(AssignmentStmt { operator, .. }) => {
                assert_eq!(operator, AssignmentOp::Equal);
            }
            other => panic!("expected Assignment, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_assignment_compound() {
        let source = "set {_x} += 1";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Assignment(AssignmentStmt { operator, .. }) => {
                assert_eq!(operator, AssignmentOp::PlusEqual);
            }
            other => panic!("expected Assignment, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Send effect
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_send_no_target() {
        let source = "send \"hello\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "send");
                assert_eq!(arguments.len(), 1);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_send_with_target() {
        let source = "send \"hello\" to player";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "send");
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_broadcast() {
        let source = "broadcast \"hi\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "broadcast");
                assert_eq!(arguments.len(), 1);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Add/Remove effect
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_add() {
        let source = "add 1 to {_x}";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "add");
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_remove() {
        let source = "remove {_x} from {_list}";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "remove");
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Effects (delete, clear, give, teleport)
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_delete() {
        let source = "delete {_x}";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "delete");
                assert_eq!(arguments.len(), 1);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_clear() {
        let source = "clear {_x}";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect, ..
            }) => {
                assert_eq!(effect, "clear");
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_give() {
        let source = "give player diamond";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "give");
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Return
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_return_no_value() {
        let source = "return";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Return(ReturnStmt { value, .. }) => {
                assert!(value.is_none());
            }
            other => panic!("expected Return, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_return_with_value() {
        let source = "return 5";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Return(ReturnStmt {
                value, span, ..
            }) => {
                assert!(value.is_some());
                assert!(span.start < span.end);
            }
            other => panic!("expected Return, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Stop / Exit / Continue
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_stop() {
        let source = "stop";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::Stop(_)));
    }

    #[test]
    fn test_parse_exit() {
        let source = "exit";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::Exit(_)));
    }

    #[test]
    fn test_parse_exit_loop() {
        let source = "exit loop";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::Exit(_)));
    }

    #[test]
    fn test_parse_continue() {
        let source = "continue";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::Continue(_)));
    }

    // -----------------------------------------------------------------------
    // Cancel event
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_cancel_event() {
        let source = "cancel event";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::CancelEvent(_)));
    }

    #[test]
    fn test_parse_cancel() {
        let source = "cancel";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::CancelEvent(_)));
    }

    // -----------------------------------------------------------------------
    // Wait
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_wait_no_duration() {
        let source = "wait";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Wait(WaitStmt { duration, .. }) => {
                assert!(duration.is_none());
            }
            other => panic!("expected Wait, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_wait_with_duration() {
        let source = "wait 5 seconds";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Wait(WaitStmt { duration, .. }) => {
                assert!(duration.is_some());
            }
            other => panic!("expected Wait, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Effect fallback
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_effect_fallback() {
        let source = "heal player";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "heal");
                assert_eq!(arguments.len(), 1);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_effect_fallback_multi_arg() {
        let source = "someEffect arg1 arg2";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::Effect(EffectStmt {
                effect,
                arguments,
                ..
            }) => {
                assert_eq!(effect, "someEffect");
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected Effect, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Block body (tested indirectly via block statements)
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_block_body_via_if() {
        let source = "if true:\n    broadcast \"hi\"\n    broadcast \"bye\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        match stmt {
            Statement::If(IfStmt { body, .. }) => {
                assert_eq!(body.len(), 2);
                assert!(matches!(body[0], Statement::Effect(_)));
                assert!(matches!(body[1], Statement::Effect(_)));
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Nested if
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_nested_if() {
        let source = "if true:\n    if false:\n        broadcast \"nested\"\n    broadcast \"outer\"";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        match stmt {
            Statement::If(IfStmt { body, .. }) => {
                assert_eq!(body.len(), 2);

                match &body[0] {
                    Statement::If(_) => {}
                    other => panic!("expected nested If, got {:?}", other),
                }
                match &body[1] {
                    Statement::Effect(_) => {}
                    other => panic!("expected Effect, got {:?}", other),
                }
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Error recovery
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_recovery_invalid_statement() {
        let source = "|";
        let (parser, stmt) = parse_stmt(source);
        assert!(!parser.errors.is_empty(), "expected errors");
        assert!(matches!(stmt, Statement::BlankLine));
    }

    #[test]
    fn test_parse_blank_line() {
        let source = "\n\n";
        let (parser, stmt) = parse_stmt(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
        assert!(matches!(stmt, Statement::BlankLine));
    }

    // -----------------------------------------------------------------------
    // Span sanity
    // -----------------------------------------------------------------------

    #[test]
    fn test_span_non_empty() {
        let cases = vec![
            "if true:\n    broadcast \"hi\"",
            "while true:\n    broadcast \"hi\"",
            "loop 5 times:\n    broadcast \"hi\"",
            "for each x in {_y}:\n    broadcast x",
            "set {_x} to 5",
            "send \"hi\"",
            "return 5",
            "stop",
            "exit",
            "continue",
            "cancel event",
            "wait",
            "heal player",
        ];
        for source in cases {
            let (parser, stmt) = parse_stmt(source);
            assert!(
                parser.errors.is_empty(),
                "errors for {:?}: {:?}",
                source,
                parser.errors
            );
            match &stmt {
                Statement::BlankLine => {}
                _ => {
                    let span_str = format!("{:?}", stmt);
                    assert!(
                        span_str.contains("span: Span"),
                        "missing span in {:?}",
                        stmt
                    );
                }
            }
        }
    }
}
