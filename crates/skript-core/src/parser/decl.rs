use crate::ast::*;
use crate::lexer::{SpannedToken, Token};
use crate::parser::stmt::parse_block_body;
use crate::parser::Parser;

// ---------------------------------------------------------------------------
// Token → text reconstruction helpers
// ---------------------------------------------------------------------------

fn token_text(token: &Token) -> String {
    match token {
        Token::Identifier(s)
        | Token::Number(s)
        | Token::String(s)
        | Token::StrText(s)
        | Token::EventValue(s) => s.clone(),
        Token::Keyword(s) => s.to_string(),
        Token::Boolean(v) => v.to_string(),
        Token::Variable(parts) => format!("{{{}}}", parts.join("::")),
        Token::MiniMessageOpen(s) => format!("<{}>", s),
        Token::MiniMessageClose(s) => format!("</{}>", s),
        Token::Plus => "+".into(),
        Token::Minus => "-".into(),
        Token::Star => "*".into(),
        Token::Slash => "/".into(),
        Token::Percent => "%".into(),
        Token::Caret => "^".into(),
        Token::Tilde => "~".into(),
        Token::EqualEqual => "==".into(),
        Token::NotEqual => "!=".into(),
        Token::LessEqual => "<=".into(),
        Token::GreaterEqual => ">=".into(),
        Token::Less => "<".into(),
        Token::Greater => ">".into(),
        Token::Equal => "=".into(),
        Token::Colon => ":".into(),
        Token::ColonEqual => ":=".into(),
        Token::PlusEqual => "+=".into(),
        Token::MinusEqual => "-=".into(),
        Token::StarEqual => "*=".into(),
        Token::SlashEqual => "/=".into(),
        Token::Dot => ".".into(),
        Token::Comma => ",".into(),
        Token::Semicolon => ";".into(),
        Token::Arrow => "->".into(),
        Token::Pipe => "|".into(),
        Token::LParen => "(".into(),
        Token::RParen => ")".into(),
        Token::LBrace => "{".into(),
        Token::RBrace => "}".into(),
        Token::LBracket => "[".into(),
        Token::RBracket => "]".into(),
        Token::StrInterpolationStart => "%{".into(),
        Token::StrInterpolationEnd => "}%".into(),
        Token::Indent => "".into(),
        Token::Dedent => "".into(),
        Token::Newline => "".into(),
        Token::Eof => "".into(),
    }
}

fn reconstruct_text(tokens: &[SpannedToken]) -> String {
    let mut text = String::new();
    for (i, tok) in tokens.iter().enumerate() {
        if i > 0 {
            let prev = &tokens[i - 1];
            if prev.span.end < tok.span.start {
                text.push(' ');
            }
        }
        text.push_str(&token_text(&tok.token));
    }
    text
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Consume an `Identifier`, `Keyword`, `Variable`, or `EventValue` token and
/// return its text.
fn parse_identifier_or_keyword(parser: &mut Parser, error_msg: &str) -> String {
    let tok = parser.peek();
    let text = match &tok.token {
        Token::Identifier(s) => s.clone(),
        Token::Keyword(s) => s.to_string(),
        Token::EventValue(s) => s.clone(),
        Token::Variable(parts) => parts.join("::"),
        _ => {
            parser.error(error_msg, tok.span);
            return String::new();
        }
    };
    parser.advance();
    text
}

/// Consume an identifier-like token and return it as a `Type` if possible.
fn parse_type(parser: &mut Parser) -> Option<Type> {
    let tok = parser.peek();
    match &tok.token {
        Token::Identifier(name) => {
            parser.advance();
            Some(Type {
                name: name.clone(),
                span: tok.span,
            })
        }
        Token::Keyword(kw) => {
            parser.advance();
            Some(Type {
                name: kw.to_string(),
                span: tok.span,
            })
        }
        _ => None,
    }
}

/// Collect all tokens up to (but not including) the next Newline / Dedent / Eof.
fn collect_line_tokens(parser: &mut Parser) -> Vec<SpannedToken> {
    let mut toks = Vec::new();
    while !matches!(
        parser.peek_token(),
        Token::Newline | Token::Dedent | Token::Eof
    ) {
        toks.push(parser.advance());
    }
    toks
}

// ---------------------------------------------------------------------------
// Function declaration
// ---------------------------------------------------------------------------

/// Parse a parameter: `identifier : type [= default]`
fn parse_parameter(parser: &mut Parser) -> Parameter {
    let start_tok = parser.peek();
    let name = parse_identifier_or_keyword(parser, "Expected identifier for parameter name");
    if name.is_empty() {
        return Parameter {
            name: String::new(),
            type_: None,
            default: None,
            span: start_tok.span,
        };
    }

    let mut type_: Option<Type> = None;
    let mut default: Option<Expression> = None;

    if matches!(parser.peek_token(), Token::Colon) {
        parser.advance();
        type_ = parse_type(parser);
    }

    if matches!(parser.peek_token(), Token::Equal) {
        parser.advance();
        let default_tokens = collect_line_tokens_until(&mut *parser, &Token::Comma);
        if !default_tokens.is_empty() {
            let content = reconstruct_text(&default_tokens);
            let span = Span::new(
                default_tokens.first().unwrap().span.start,
                default_tokens.last().unwrap().span.end,
            );
            default = Some(Expression::Unknown(UnknownExpr { content, span }));
        }
    }

    Parameter {
        name,
        type_,
        default,
        span: Span::new(start_tok.span.start, parser.peek().span.start),
    }
}

/// Collect tokens until we see `stop_at` or Newline/Dedent/Eof.
fn collect_line_tokens_until(parser: &mut Parser, stop_at: &Token) -> Vec<SpannedToken> {
    let mut toks = Vec::new();
    loop {
        let tok = parser.peek_token();
        if tok == *stop_at || matches!(tok, Token::Newline | Token::Dedent | Token::Eof) {
            break;
        }
        toks.push(parser.advance());
    }
    toks
}

/// Parse a function declaration.
///
/// ```skript
/// function name(params) [:: return_type] [: | :]
///     body
/// ```
pub fn parse_function(parser: &mut Parser) -> FunctionDecl {
    let start = parser.peek().span;
    parser.advance();

    let name = parse_identifier_or_keyword(parser, "Expected function name");

    parser.expect(&Token::LParen);

    let mut parameters = Vec::new();
    if !matches!(parser.peek_token(), Token::RParen) {
        loop {
            parameters.push(parse_parameter(parser));
            if !parser.consume_if(&Token::Comma) {
                break;
            }
        }
    }
    parser.expect(&Token::RParen);

    let return_type = if parser.consume_if(&Token::Arrow) {
        parse_type(parser)
    } else if matches!(parser.peek_token(), Token::Colon) {
        if let Some(next) = parser.peek_n(1) {
            if matches!(next.token, Token::Colon) {
                parser.advance();
                parser.advance();
                parse_type(parser)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    FunctionDecl {
        name,
        parameters,
        return_type,
        body,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Command declaration
// ---------------------------------------------------------------------------

/// Parse command arguments (`<arg>` or `[<arg>]`).
fn parse_command_arg(parser: &mut Parser) -> CommandArg {
    let optional = parser.consume_if(&Token::LBracket);

    parser.expect(&Token::Less);
    let name_tok = parser.peek();
    let name = match &name_tok.token {
        Token::Identifier(n) => n.clone(),
        _ => {
            parser.error(
                "Expected identifier for command argument name",
                name_tok.span,
            );
            String::new()
        }
    };
    parser.advance();

    let type_: Option<Type> = if parser.consume_if(&Token::Colon) {
        parse_type(parser)
    } else {
        None
    };

    parser.expect(&Token::Greater);

    if optional {
        parser.expect(&Token::RBracket);
    }

    CommandArg {
        name,
        type_,
        optional,
        span: Span::new(name_tok.span.start, parser.peek().span.start),
    }
}

fn parse_command_name(parser: &mut Parser) -> String {
    let mut name = String::new();
    if parser.consume_if(&Token::Slash) {
        name.push('/');
    }
    let ident = parse_identifier_or_keyword(parser, "Expected command name identifier");
    name.push_str(&ident);
    name
}

/// Parse a command declaration.
///
/// ```skript
/// command /name [args]:
///     metadata_key: value
///     ...
///     trigger:
///         body...
/// ```
pub fn parse_command(parser: &mut Parser) -> CommandDecl {
    let start = parser.peek().span;
    parser.advance();

    let name = parse_command_name(parser);

    let mut arguments = Vec::new();
    loop {
        let next = parser.peek_token();
        match next {
            Token::Less => {
                arguments.push(parse_command_arg(parser));
            }
            Token::LBracket => {
                arguments.push(parse_command_arg(parser));
            }
            _ => break,
        }
    }

    parser.expect(&Token::Colon);

    if matches!(parser.peek_token(), Token::Newline) {
        parser.advance();
    }

    let mut metadata = CommandMetadata::default();
    let mut body = Vec::new();

    if parser.consume_if(&Token::Indent) {
        loop {
            if matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
                break;
            }

            if matches!(parser.peek_token(), Token::Keyword(k) if k == "trigger") {
                parser.advance();
                parser.expect(&Token::Colon);
                body = parse_block_body(parser);
                break;
            }

            let key = parse_identifier_or_keyword(parser, "Expected metadata key");

            parser.expect(&Token::Colon);
            let value_tokens = collect_line_tokens(parser);
            let value = reconstruct_text(&value_tokens);

            set_metadata_field(&mut metadata, &key, &value);

            if matches!(parser.peek_token(), Token::Newline) {
                parser.advance();
            }
        }

        if body.is_empty() && !matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
            body = parse_block_body(parser);
        }

        parser.expect(&Token::Dedent);
    }

    CommandDecl {
        name,
        arguments,
        metadata,
        body,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

fn set_metadata_field(metadata: &mut CommandMetadata, key: &str, value: &str) {
    match key.to_lowercase().as_str() {
        "description" => metadata.description = Some(value.to_string()),
        "executable by" | "executableby" => metadata.executable_by = Some(value.to_string()),
        "permission" => metadata.permission = Some(value.to_string()),
        "permission message" | "permissionmessage" => {
            metadata.permission_message = Some(value.to_string())
        }
        "cooldown" => metadata.cooldown = Some(value.to_string()),
        "cooldown message" | "cooldownmessage" => {
            metadata.cooldown_message = Some(value.to_string())
        }
        "cooldown bypass" | "cooldownbypass" => metadata.cooldown_bypass = Some(value.to_string()),
        "aliases" => metadata.aliases = Some(value.to_string()),
        "usage" => metadata.usage = Some(value.to_string()),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Event declaration
// ---------------------------------------------------------------------------

/// Parse an event declaration.
///
/// ```skript
/// on event_type [conditions]:
///     body...
/// ```
pub fn parse_event(parser: &mut Parser) -> EventDecl {
    let start = parser.peek().span;
    parser.advance();

    let mut parts = Vec::new();
    loop {
        match parser.peek_token() {
            Token::Identifier(ref s) => {
                parts.push(s.clone());
                parser.advance();
            }
            Token::Keyword(ref k) if *k != "on" => {
                parts.push(k.to_string());
                parser.advance();
            }
            _ => break,
        }
    }
    let event_type = parts.join(" ");

    let conditions = Vec::new();

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    EventDecl {
        event_type,
        conditions,
        body,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Periodic event declaration
// ---------------------------------------------------------------------------

/// Parse a periodic event declaration.
///
/// ```skript
/// every period:
///     body...
/// ```
pub fn parse_periodic_event(parser: &mut Parser) -> PeriodicEventDecl {
    let start = parser.peek().span;
    parser.advance();

    let period_tokens = collect_line_tokens_until(parser, &Token::Colon);
    let period = reconstruct_text(&period_tokens);

    parser.expect(&Token::Colon);
    let body = parse_block_body(parser);

    PeriodicEventDecl {
        period,
        body,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Options block
// ---------------------------------------------------------------------------

/// Parse an options block.
///
/// ```skript
/// options:
///     key: value
/// ```
pub fn parse_options_block(parser: &mut Parser) -> OptionsBlock {
    let start = parser.peek().span;
    parser.advance();
    parser.expect(&Token::Colon);

    let mut entries = Vec::new();

    if matches!(parser.peek_token(), Token::Newline) {
        parser.advance();
    }
    if parser.consume_if(&Token::Indent) {
        while !matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
            let entry_start = parser.peek().span.start;
            let key = parse_identifier_or_keyword(parser, "Expected option key");

            parser.expect(&Token::Colon);
            let value_tokens = collect_line_tokens(parser);
            let value = reconstruct_text(&value_tokens);

            entries.push(OptionEntry {
                key,
                value,
                span: Span::new(entry_start, parser.peek().span.start),
            });

            if matches!(parser.peek_token(), Token::Newline) {
                parser.advance();
            }
        }
        parser.expect(&Token::Dedent);
    }

    OptionsBlock {
        entries,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Aliases block
// ---------------------------------------------------------------------------

/// Parse an aliases block.
///
/// ```skript
/// aliases:
///     name = value
/// ```
pub fn parse_aliases_block(parser: &mut Parser) -> AliasesBlock {
    let start = parser.peek().span;
    parser.advance();
    parser.expect(&Token::Colon);

    let mut entries = Vec::new();

    if matches!(parser.peek_token(), Token::Newline) {
        parser.advance();
    }
    if parser.consume_if(&Token::Indent) {
        while !matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
            let entry_start = parser.peek().span.start;
            let source = parse_identifier_or_keyword(parser, "Expected alias source");

            parser.expect(&Token::Equal);
            let target_tokens = collect_line_tokens(parser);
            let target = reconstruct_text(&target_tokens);

            entries.push(AliasEntry {
                source,
                target,
                span: Span::new(entry_start, parser.peek().span.start),
            });

            if matches!(parser.peek_token(), Token::Newline) {
                parser.advance();
            }
        }
        parser.expect(&Token::Dedent);
    }

    AliasesBlock {
        entries,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Variables block
// ---------------------------------------------------------------------------

/// Parse a variables block.
///
/// ```skript
/// variables:
///     name: type = value
/// ```
pub fn parse_variables_block(parser: &mut Parser) -> VariablesBlock {
    let start = parser.peek().span;
    parser.advance();
    parser.expect(&Token::Colon);

    let mut entries = Vec::new();

    if matches!(parser.peek_token(), Token::Newline) {
        parser.advance();
    }
    if parser.consume_if(&Token::Indent) {
        while !matches!(parser.peek_token(), Token::Dedent | Token::Eof) {
            let entry_start = parser.peek().span.start;
            let name = parse_identifier_or_keyword(parser, "Expected variable name");

            let type_: Option<Type> = if parser.consume_if(&Token::Colon) {
                parse_type(parser)
            } else {
                None
            };

            let initializer: Option<Expression> = if parser.consume_if(&Token::Equal) {
                let init_tokens = collect_line_tokens(parser);
                if !init_tokens.is_empty() {
                    let content = reconstruct_text(&init_tokens);
                    let span = Span::new(
                        init_tokens.first().unwrap().span.start,
                        init_tokens.last().unwrap().span.end,
                    );
                    Some(Expression::Unknown(UnknownExpr { content, span }))
                } else {
                    None
                }
            } else {
                None
            };

            entries.push(VariableDecl {
                name,
                type_,
                initializer,
                span: Span::new(entry_start, parser.peek().span.start),
            });

            if matches!(parser.peek_token(), Token::Newline) {
                parser.advance();
            }
        }
        parser.expect(&Token::Dedent);
    }

    VariablesBlock {
        entries,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Using directive
// ---------------------------------------------------------------------------

/// Parse a `using` directive.
///
/// ```skript
/// using identifier
/// ```
pub fn parse_using_directive(parser: &mut Parser) -> UsingDirective {
    let start = parser.peek().span;
    parser.advance();

    let what_tokens = collect_line_tokens(parser);
    let what = reconstruct_text(&what_tokens);

    UsingDirective {
        what,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Example block
// ---------------------------------------------------------------------------

/// Parse an example block.
///
/// ```skript
/// example:
///     content...
/// ```
pub fn parse_example_block(parser: &mut Parser) -> ExampleBlock {
    let start = parser.peek().span;
    parser.advance();
    parser.expect(&Token::Colon);
    let content = parse_block_body(parser);

    ExampleBlock {
        content,
        span: Span::new(start.start, parser.peek().span.start),
    }
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Parse a single top-level item based on the current token.
///
/// Returns `None` if the current token does not start a declaration
/// (e.g., structural tokens, Eof, or tokens handled by parent parsers).
///
/// # Note
///
/// Comments (`# ...`) are skipped by the lexer and never reach the parser.
/// This is a known limitation that requires lexer changes to support
/// comment tokens.
pub fn parse_top_level_item(parser: &mut Parser) -> Option<TopLevelItem> {
    match parser.peek_token() {
        Token::Keyword("function") => Some(TopLevelItem::Function(parse_function(parser))),
        Token::Keyword("command") => Some(TopLevelItem::Command(parse_command(parser))),
        Token::Keyword("on") => Some(TopLevelItem::Event(parse_event(parser))),
        Token::Keyword("every") => Some(TopLevelItem::PeriodicEvent(parse_periodic_event(parser))),
        Token::Keyword("options") => Some(TopLevelItem::Options(parse_options_block(parser))),
        Token::Keyword("aliases") => Some(TopLevelItem::Aliases(parse_aliases_block(parser))),
        Token::Keyword("variables") => Some(TopLevelItem::Variables(parse_variables_block(parser))),
        Token::Keyword("using") => Some(TopLevelItem::Using(parse_using_directive(parser))),
        Token::Keyword("example") => Some(TopLevelItem::Example(parse_example_block(parser))),
        Token::Indent | Token::Dedent | Token::Newline | Token::Eof => None,
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

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

    fn parse_tokens(source: &str) -> (Parser, Option<TopLevelItem>) {
        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let item = parse_top_level_item(&mut parser);
        (parser, item)
    }

    // -----------------------------------------------------------------------
    // 1. Empty / no tokens
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_empty() {
        let (parser, item) = parse_tokens("");
        assert!(item.is_none());
        assert!(parser.errors.is_empty());
    }

    // -----------------------------------------------------------------------
    // 2. Function header (no body)
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_function_header() {
        let (parser, item) = parse_tokens("function greet(name: text):");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let func = match item {
            Some(TopLevelItem::Function(f)) => f,
            other => panic!("Expected Function, got {:?}", other),
        };

        assert_eq!(func.name, "greet");
        assert_eq!(func.parameters.len(), 1);
        assert_eq!(func.parameters[0].name, "name");
        assert_eq!(
            func.parameters[0].type_.as_ref().map(|t| t.name.as_str()),
            Some("text")
        );
        assert!(func.return_type.is_none());
        assert!(func.body.is_empty());
    }

    // -----------------------------------------------------------------------
    // 3. Function with return type
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_function_return_type() {
        let (parser, item) = parse_tokens("function add(a: number, b: number) :: number:");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let func = match item {
            Some(TopLevelItem::Function(f)) => f,
            other => panic!("Expected Function, got {:?}", other),
        };

        assert_eq!(func.name, "add");
        assert_eq!(func.parameters.len(), 2);
        assert_eq!(func.parameters[0].name, "a");
        assert_eq!(
            func.parameters[0].type_.as_ref().map(|t| t.name.as_str()),
            Some("number")
        );
        assert_eq!(func.parameters[1].name, "b");
        assert_eq!(
            func.parameters[1].type_.as_ref().map(|t| t.name.as_str()),
            Some("number")
        );
        assert_eq!(
            func.return_type.as_ref().map(|t| t.name.as_str()),
            Some("number")
        );
        assert!(func.body.is_empty());
    }

    // -----------------------------------------------------------------------
    // 4. Command (simple, no args)
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_command_simple() {
        let (parser, item) = parse_tokens("command /hello:");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let cmd = match item {
            Some(TopLevelItem::Command(c)) => c,
            other => panic!("Expected Command, got {:?}", other),
        };

        assert_eq!(cmd.name, "/hello");
        assert!(cmd.arguments.is_empty());
        assert!(cmd.body.is_empty());
    }

    // -----------------------------------------------------------------------
    // 5. Command with args
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_command_with_args() {
        let (parser, item) = parse_tokens("command /give <player> [<amount>]:");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let cmd = match item {
            Some(TopLevelItem::Command(c)) => c,
            other => panic!("Expected Command, got {:?}", other),
        };

        assert_eq!(cmd.name, "/give");
        assert_eq!(
            cmd.arguments.len(),
            2,
            "expected 2 args, got {:?}",
            cmd.arguments
        );

        assert_eq!(cmd.arguments[0].name, "player");
        assert!(!cmd.arguments[0].optional);

        assert_eq!(cmd.arguments[1].name, "amount");
        assert!(cmd.arguments[1].optional);
    }

    // -----------------------------------------------------------------------
    // 6. Command with metadata and trigger
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_command_metadata() {
        let source = "command /foo:\n\tpermission: foo.bar\n\ttrigger:\n\t\tsend \"hi\"";
        let (parser, item) = parse_tokens(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let cmd = match item {
            Some(TopLevelItem::Command(c)) => c,
            other => panic!("Expected Command, got {:?}", other),
        };

        assert_eq!(cmd.name, "/foo");
        assert_eq!(
            cmd.metadata.permission.as_deref(),
            Some("foo.bar"),
            "expected foo.bar, got {:?}",
            cmd.metadata.permission
        );
        assert!(!cmd.body.is_empty(), "expected trigger body");
    }

    // -----------------------------------------------------------------------
    // 7. Event
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_event() {
        let (parser, item) = parse_tokens("on first join:");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let event = match item {
            Some(TopLevelItem::Event(e)) => e,
            other => panic!("Expected Event, got {:?}", other),
        };

        assert_eq!(event.event_type, "first join");
        assert!(event.conditions.is_empty());
        assert!(event.body.is_empty());
    }

    // -----------------------------------------------------------------------
    // 8. Periodic event
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_periodic_event() {
        let (parser, item) = parse_tokens("every 10 seconds:");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let pe = match item {
            Some(TopLevelItem::PeriodicEvent(p)) => p,
            other => panic!("Expected PeriodicEvent, got {:?}", other),
        };

        assert_eq!(pe.period, "10 seconds");
        assert!(pe.body.is_empty());
    }

    // -----------------------------------------------------------------------
    // 9. Options block
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_options() {
        let source = "options:\n\tkey1: value1\n\tkey2: value2";
        let (parser, item) = parse_tokens(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let opts = match item {
            Some(TopLevelItem::Options(o)) => o,
            other => panic!("Expected Options, got {:?}", other),
        };

        assert_eq!(opts.entries.len(), 2);
        assert_eq!(opts.entries[0].key, "key1");
        assert_eq!(opts.entries[0].value, "value1");
        assert_eq!(opts.entries[1].key, "key2");
        assert_eq!(opts.entries[1].value, "value2");
    }

    // -----------------------------------------------------------------------
    // 10. Aliases block
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_aliases() {
        let source = "aliases:\n\tname1 = value1\n\tname2 = value2";
        let (parser, item) = parse_tokens(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let aliases = match item {
            Some(TopLevelItem::Aliases(a)) => a,
            other => panic!("Expected Aliases, got {:?}", other),
        };

        assert_eq!(aliases.entries.len(), 2);
        assert_eq!(aliases.entries[0].source, "name1");
        assert_eq!(aliases.entries[0].target, "value1");
        assert_eq!(aliases.entries[1].source, "name2");
        assert_eq!(aliases.entries[1].target, "value2");
    }

    // -----------------------------------------------------------------------
    // 11. Top-level dispatch (multiple items)
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_top_level_dispatch() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"\n\ncommand /bye:\n\ttrigger:\n\t\tsend \"bye\"";
        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);

        let item1 = parse_top_level_item(&mut parser);
        assert!(
            matches!(item1, Some(TopLevelItem::Function(_))),
            "expected Function, got {:?}",
            item1
        );

        let item2 = parse_top_level_item(&mut parser);
        assert!(
            matches!(item2, Some(TopLevelItem::Command(_))),
            "expected Command, got {:?}",
            item2
        );

        let item3 = parse_top_level_item(&mut parser);
        assert!(item3.is_none(), "expected None (Eof), got {:?}", item3);

        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);
    }

    // -----------------------------------------------------------------------
    // 12. Error recovery
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_recovery() {
        // Invalid syntax: "function 123(name):" — number instead of name
        let (parser, item) = parse_tokens("function 123(name):");
        // Should still produce a FunctionDecl with empty name, and collect errors
        assert!(
            matches!(item, Some(TopLevelItem::Function(_))),
            "expected Function, got {:?}",
            item
        );
        assert!(!parser.errors.is_empty(), "expected errors");
    }

    // -----------------------------------------------------------------------
    // 13. Using directive
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_using() {
        let (parser, item) = parse_tokens("using skript");
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let using = match item {
            Some(TopLevelItem::Using(u)) => u,
            other => panic!("Expected Using, got {:?}", other),
        };

        assert_eq!(using.what, "skript");
    }

    // -----------------------------------------------------------------------
    // 14. Example block
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_example() {
        let source = "example:\n\tdo something";
        let (parser, item) = parse_tokens(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let example = match item {
            Some(TopLevelItem::Example(e)) => e,
            other => panic!("Expected Example, got {:?}", other),
        };

        assert_eq!(example.content.len(), 1);
    }

    // -----------------------------------------------------------------------
    // 15. Variables block
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_variables() {
        let source = "variables:\n\tname: text = \"hello\"";
        let (parser, item) = parse_tokens(source);
        assert!(parser.errors.is_empty(), "errors: {:?}", parser.errors);

        let vars = match item {
            Some(TopLevelItem::Variables(v)) => v,
            other => panic!("Expected Variables, got {:?}", other),
        };

        assert_eq!(vars.entries.len(), 1);
        assert_eq!(vars.entries[0].name, "name");
        assert_eq!(
            vars.entries[0].type_.as_ref().map(|t| t.name.as_str()),
            Some("text")
        );
        assert!(vars.entries[0].initializer.is_some());
    }
}
