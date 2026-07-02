use crate::ast::Span;
use crate::lexer::{SpannedToken, Token};

/// An error produced during parsing with a message and source location.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
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
