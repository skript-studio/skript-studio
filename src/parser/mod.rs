use crate::ast::Span;
use crate::lexer::{SpannedToken, Token};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

pub struct Parser {
    pub tokens: Vec<SpannedToken>,
    pub pos: usize,
    pub errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    pub fn peek(&self) -> SpannedToken {
        self.tokens.get(self.pos).cloned().unwrap_or_else(|| SpannedToken {
            token: Token::Eof,
            span: Span::new(0, 0),
        })
    }

    pub fn peek_token(&self) -> Token {
        self.peek().token
    }

    pub fn peek_n(&self, n: usize) -> Option<SpannedToken> {
        self.tokens.get(self.pos + n).cloned()
    }

    pub fn advance(&mut self) -> SpannedToken {
        let tok = self.peek();
        self.pos += 1;
        tok
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

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
}

pub mod decl;
pub use decl::parse_top_level_item;
