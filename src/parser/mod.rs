use crate::ast::{Program, Span};
use crate::lexer::{Lexer, SpannedToken, Token};

/// An error produced during parsing with a message and source location.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// The result of parsing a Skript document.
pub struct ParseResult {
    pub program: Program,
    pub errors: Vec<ParseError>,
}

/// Tokenize and parse a complete Skript document.
pub fn parse_program(source: &str) -> ParseResult {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token();
        let is_eof = matches!(tok.token, Token::Eof);
        tokens.push(tok);
        if is_eof {
            break;
        }
    }

    let mut parser = Parser::new(tokens);
    let mut items = Vec::new();

    while !parser.is_at_end() {
        while matches!(parser.peek_token(), Token::Newline | Token::Dedent) {
            parser.advance();
        }
        if parser.is_at_end() {
            break;
        }

        match parse_top_level_item(&mut parser) {
            Some(item) => items.push(item),
            None => {
                parser.sync();
            }
        }
    }

    ParseResult {
        program: Program { items },
        errors: parser.errors,
    }
}

/// A recursive-descent parser over a token stream.
pub struct Parser {
    pub tokens: Vec<SpannedToken>,
    pub pos: usize,
    pub errors: Vec<ParseError>,
}

impl Parser {
    /// Create a new `Parser` from a vector of tokens.
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    /// Return the current token without consuming it. Returns Eof at end of input.
    pub fn peek(&self) -> SpannedToken {
        self.tokens.get(self.pos).cloned().unwrap_or_else(|| SpannedToken {
            token: Token::Eof,
            span: Span::new(0, 0),
        })
    }

    /// Return the token kind of the current token without consuming it.
    pub fn peek_token(&self) -> Token {
        self.peek().token
    }

    /// Look ahead `n` tokens without consuming. Returns `None` past the end.
    pub fn peek_n(&self, n: usize) -> Option<SpannedToken> {
        self.tokens.get(self.pos + n).cloned()
    }

    /// Consume and return the current token. Returns Eof past end of input.
    pub fn advance(&mut self) -> SpannedToken {
        let tok = self.peek();
        self.pos += 1;
        tok
    }

    /// Consume and return the current token. Panics at end of input.
    pub fn next(&mut self) -> SpannedToken {
        if self.is_at_end() {
            panic!("called Parser::next at end of input");
        }
        self.advance()
    }

    /// Consume the current token if it matches `token`.
    pub fn consume_if(&mut self, token: &Token) -> bool {
        if self.peek_token() == *token {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Expect the current token to match `token`. If so, consume and return
    /// its span. Otherwise, record an error and return a zero span.
    pub fn expect(&mut self, expected: &Token) -> Span {
        if self.peek_token() == *expected {
            let tok = self.advance();
            tok.span
        } else {
            let span = self.peek().span;
            self.error(
                format!("Expected {:?}, got {:?}", expected, self.peek_token()),
                span,
            );
            Span::new(0, 0)
        }
    }

    /// Record a `ParseError` with the given message and span.
    pub fn error(&mut self, message: impl Into<String>, span: Span) {
        self.errors.push(ParseError {
            message: message.into(),
            span,
        });
    }

    /// Skip tokens until a synchronisation point (Newline, Dedent, Eof).
    pub fn sync(&mut self) {
        loop {
            match self.peek_token() {
                Token::Newline | Token::Dedent | Token::Eof => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// True if the current token is Eof.
    pub fn is_at_end(&self) -> bool {
        matches!(self.peek_token(), Token::Eof)
    }

    /// Return the list of `ParseError`s recorded so far.
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
}

pub mod decl;
pub use decl::parse_top_level_item;

pub mod expr;
pub use expr::parse_expression;

pub mod stmt;
pub use stmt::{parse_block_body, parse_statement};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Statement, TopLevelItem};

    #[test]
    fn test_parse_empty_source() {
        let result = parse_program("");
        assert!(result.program.items.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_parse_only_comments() {
        let result = parse_program("# comment 1\n# comment 2\n");
        assert!(result.program.items.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_parse_only_whitespace() {
        let result = parse_program("  \n  \n");
        assert!(result.program.items.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_parse_function_declaration() {
        let source = "function greet(name: text):\n\tbroadcast \"hello\"";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 1);
        match &result.program.items[0] {
            TopLevelItem::Function(f) => {
                assert_eq!(f.name, "greet");
                assert_eq!(f.parameters.len(), 1);
                assert_eq!(f.body.len(), 1);
            }
            other => panic!("expected Function, got {:?}", other),
        }
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_parse_command_declaration() {
        let source = "command /hello <text>:\n\ttrigger:\n\t\tsend \"hi\"";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 1);
        match &result.program.items[0] {
            TopLevelItem::Command(c) => {
                assert_eq!(c.name, "/hello");
                assert_eq!(c.arguments.len(), 1);
                assert_eq!(c.body.len(), 1);
            }
            other => panic!("expected Command, got {:?}", other),
        }
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_parse_event_declaration() {
        let source = "on first join:\n\tbroadcast \"welcome\"";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 1);
        match &result.program.items[0] {
            TopLevelItem::Event(e) => {
                assert_eq!(e.event_type, "first join");
                assert_eq!(e.body.len(), 1);
            }
            other => panic!("expected Event, got {:?}", other),
        }
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_parse_multiple_top_level_items() {
        let source = "function foo():\n\treturn\n\ncommand /bar:\n\ttrigger:\n\t\tsend \"ok\"";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 2);
        assert!(matches!(result.program.items[0], TopLevelItem::Function(_)));
        assert!(matches!(result.program.items[1], TopLevelItem::Command(_)));
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_parse_if_inside_function() {
        let source = "function test():\n\tif true:\n\t\tsend \"yes\"\n\telse:\n\t\tsend \"no\"";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 1);
        match &result.program.items[0] {
            TopLevelItem::Function(f) => {
                assert_eq!(f.body.len(), 1);
                match &f.body[0] {
                    Statement::If(if_stmt) => {
                        assert!(if_stmt.else_branch.is_some());
                    }
                    other => panic!("expected If, got {:?}", other),
                }
            }
            other => panic!("expected Function, got {:?}", other),
        }
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_parse_options_block() {
        let source = "options:\n\tkey1: val1\n\tkey2: val2";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 1);
        match &result.program.items[0] {
            TopLevelItem::Options(o) => {
                assert_eq!(o.entries.len(), 2);
            }
            other => panic!("expected Options, got {:?}", other),
        }
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_parse_invalid_syntax_recovered() {
        let source = "some random stuff\nfunction foo():\n\treturn";
        let result = parse_program(source);
        // sync() silently recovers from unrecognized top-level tokens
        assert_eq!(result.program.items.len(), 1);
        assert!(matches!(result.program.items[0], TopLevelItem::Function(_)));
    }

    #[test]
    fn test_parse_errors_collected() {
        let source = "function foo():\n\t|\n\treturn";
        let result = parse_program(source);
        assert!(!result.errors.is_empty(), "expected errors for invalid token inside body");
        assert_eq!(result.program.items.len(), 1);
    }

    #[test]
    fn test_parse_empty_lines_between_items() {
        let source = "function a():\n\treturn\n\n\n\nfunction b():\n\treturn";
        let result = parse_program(source);
        assert_eq!(result.program.items.len(), 2);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn test_no_panic_on_garbage_input() {
        let inputs = [
            "||||",
            "!@#$%^&*()",
            ";;;\n;;;",
            "a b c d e f g",
            "{{{{{{{",
            "\n\n\n\n\n",
        ];
        for input in &inputs {
            let result = parse_program(input);
            // Must not panic; errors are acceptable
            let _ = result;
        }
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use crate::ast::TopLevelItem;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("test-fixtures")
            .join("SKRIPT_CODE")
            .join(name)
    }

    fn parse_fixture(name: &str) -> ParseResult {
        let path = fixture_path(name);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {name}: {e}"));
        parse_program(&content)
    }

    #[test]
    fn test_parse_functions_sk() {
        let result = parse_fixture("functions.sk");
        assert!(!result.program.items.is_empty(), "expected items in functions.sk");
        let has_func = result
            .program
            .items
            .iter()
            .any(|i| matches!(i, TopLevelItem::Function(_)));
        assert!(has_func, "expected at least one function");
    }

    #[test]
    fn test_parse_events_sk() {
        let result = parse_fixture("events.sk");
        assert!(!result.program.items.is_empty(), "expected items in events.sk");
        let has_cmd = result
            .program
            .items
            .iter()
            .any(|i| matches!(i, TopLevelItem::Command(_)));
        assert!(has_cmd, "expected at least one command");
    }

    #[test]
    fn test_parse_commands_sk() {
        let result = parse_fixture("commands.sk");
        assert!(!result.program.items.is_empty(), "expected items in commands.sk");
    }

    #[test]
    fn test_parse_loops_sk() {
        let result = parse_fixture("loops.sk");
        assert!(!result.program.items.is_empty(), "expected items in loops.sk");
    }

    #[test]
    fn test_parse_variables_sk() {
        let result = parse_fixture("variables.sk");
        assert!(!result.program.items.is_empty(), "expected items in variables.sk");
    }

    #[test]
    fn test_parse_options_meta_sk() {
        let result = parse_fixture("options and meta.sk");
        assert!(!result.program.items.is_empty(), "expected items in options and meta.sk");
    }

    #[test]
    fn test_parse_chest_menus_sk() {
        let result = parse_fixture("chest menus.sk");
        assert!(!result.program.items.is_empty(), "expected items in chest menus.sk");
    }

    #[test]
    fn test_parse_text_formatting_sk() {
        let result = parse_fixture("text formatting.sk");
        assert!(!result.program.items.is_empty(), "expected items in text formatting.sk");
    }

    #[test]
    fn test_parse_timings_sk() {
        let result = parse_fixture("timings.sk");
        assert!(!result.program.items.is_empty(), "expected items in timings.sk");
    }
}
