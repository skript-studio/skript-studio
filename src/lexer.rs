//! Indentation-aware lexer for the Skript language.
//!
//! Produces a flat token stream with [`Indent`](Token::Indent) /
//! [`Dedent`](Token::Dedent) markers so the parser does not need to
//! track indentation itself.
//!
//! ## Modes
//!
//! The lexer has three modes:
//!
//! * **Normal** – tokenizes Skript source (keywords, identifiers, numbers,
//!   operators, variables, strings, …).
//! * **InString** – inside a `"..."` literal.  Accumulates text and emits
//!   [`StrText`](Token::StrText), [`StrInterpolationStart`](Token::StrInterpolationStart) /
//!   [`StrInterpolationEnd`](Token::StrInterpolationEnd), and
//!   [`MiniMessageOpen`](Token::MiniMessageOpen) /
//!   [`MiniMessageClose`](Token::MiniMessageClose) tokens.
//! * **InStringInterpolation** – inside `%{ … }%` within a string.
//!   Tokenizes a Skript expression and tracks `{}` depth to detect the
//!   closing `}%` marker.

use crate::ast::Span;

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Structural
    Indent,
    Dedent,
    Newline,
    Eof,

    // Keywords (canonical lowercase form stored as &'static str)
    Keyword(&'static str),

    // Literals
    Number(String),
    Boolean(bool),

    // Identifiers & variables
    Identifier(String),
    Variable(Vec<String>),
    EventValue(String),

    // String (complete, no interpolation)
    String(String),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Tilde,
    EqualEqual,
    NotEqual,
    LessEqual,
    GreaterEqual,
    Less,
    Greater,
    Equal,
    Colon,
    ColonEqual,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    Dot,
    Comma,
    Semicolon,
    Arrow,

    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    // String interpolation parts
    StrText(String),
    StrInterpolationStart,
    StrInterpolationEnd,
    MiniMessageOpen(String),
    MiniMessageClose(String),
}

/// A token paired with its source [`Span`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}
// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// The lexer's operational mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexMode {
    Normal,
    InString,
    InStringInterpolation,
}

/// Indentation-aware Skript lexer.
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    indent_stack: Vec<usize>,
    pending_dedents: usize,
    at_line_start: bool,
    mode: LexMode,
    interp_depth: u32,
    string_buf: String,
    has_interp: bool,
    pending: Option<SpannedToken>,
}

// ---------------------------------------------------------------------------
// Keyword matching
// ---------------------------------------------------------------------------

/// Canonical keyword names indexed by lowercased form.
fn match_keyword(text: &str) -> Option<&'static str> {
    match text.len() {
        2 => {
            if text.eq_ignore_ascii_case("on") { return Some("on"); }
            if text.eq_ignore_ascii_case("if") { return Some("if"); }
            if text.eq_ignore_ascii_case("to") { return Some("to"); }
            if text.eq_ignore_ascii_case("of") { return Some("of"); }
            if text.eq_ignore_ascii_case("in") { return Some("in"); }
            if text.eq_ignore_ascii_case("by") { return Some("by"); }
            if text.eq_ignore_ascii_case("as") { return Some("as"); }
            if text.eq_ignore_ascii_case("do") { return Some("do"); }
            if text.eq_ignore_ascii_case("is") { return Some("is"); }
            if text.eq_ignore_ascii_case("at") { return Some("at"); }
            if text.eq_ignore_ascii_case("or") { return Some("or"); }
            if text.eq_ignore_ascii_case("no") { return Some("no"); }
        }
        3 => {
            if text.eq_ignore_ascii_case("set") { return Some("set"); }
            if text.eq_ignore_ascii_case("add") { return Some("add"); }
            if text.eq_ignore_ascii_case("not") { return Some("not"); }
            if text.eq_ignore_ascii_case("for") { return Some("for"); }
            if text.eq_ignore_ascii_case("and") { return Some("and"); }
            if text.eq_ignore_ascii_case("now") { return Some("now"); }
            if text.eq_ignore_ascii_case("yes") { return Some("yes"); }
            if text.eq_ignore_ascii_case("run") { return Some("run"); }
        }
        4 => {
            if text.eq_ignore_ascii_case("else") { return Some("else"); }
            if text.eq_ignore_ascii_case("loop") { return Some("loop"); }
            if text.eq_ignore_ascii_case("from") { return Some("from"); }
            if text.eq_ignore_ascii_case("into") { return Some("into"); }
            if text.eq_ignore_ascii_case("with") { return Some("with"); }
            if text.eq_ignore_ascii_case("stop") { return Some("stop"); }
            if text.eq_ignore_ascii_case("exit") { return Some("exit"); }
            if text.eq_ignore_ascii_case("wait") { return Some("wait"); }
            if text.eq_ignore_ascii_case("send") { return Some("send"); }
            if text.eq_ignore_ascii_case("make") { return Some("make"); }
            if text.eq_ignore_ascii_case("heal") { return Some("heal"); }
            if text.eq_ignore_ascii_case("play") { return Some("play"); }
            if text.eq_ignore_ascii_case("give") { return Some("give"); }
        }
        5 => {
            if text.eq_ignore_ascii_case("while") { return Some("while"); }
            if text.eq_ignore_ascii_case("after") { return Some("after"); }
            if text.eq_ignore_ascii_case("every") { return Some("every"); }
            if text.eq_ignore_ascii_case("clear") { return Some("clear"); }
            if text.eq_ignore_ascii_case("using") { return Some("using"); }
            if text.eq_ignore_ascii_case("named") { return Some("named"); }
            if text.eq_ignore_ascii_case("where") { return Some("where"); }
            if text.eq_ignore_ascii_case("spawn") { return Some("spawn"); }
            if text.eq_ignore_ascii_case("erase") { return Some("erase"); }
            // isn't is 5 chars
            if text.eq_ignore_ascii_case("isn't") { return Some("isn't"); }
        }
        6 => {
            if text.eq_ignore_ascii_case("remove") { return Some("remove"); }
            if text.eq_ignore_ascii_case("return") { return Some("return"); }
            if text.eq_ignore_ascii_case("cancel") { return Some("cancel"); }
            if text.eq_ignore_ascii_case("damage") { return Some("damage"); }
            if text.eq_ignore_ascii_case("before") { return Some("before"); }
            if text.eq_ignore_ascii_case("delete") { return Some("delete"); }
        }
        7 => {
            if text.eq_ignore_ascii_case("trigger") { return Some("trigger"); }
            if text.eq_ignore_ascii_case("command") { return Some("command"); }
            if text.eq_ignore_ascii_case("options") { return Some("options"); }
            if text.eq_ignore_ascii_case("execute") { return Some("execute"); }
            if text.eq_ignore_ascii_case("aliases") { return Some("aliases"); }
            if text.eq_ignore_ascii_case("example") { return Some("example"); }
        }
        8 => {
            if text.eq_ignore_ascii_case("continue") { return Some("continue"); }
            if text.eq_ignore_ascii_case("function") { return Some("function"); }
            // teleport is 8 chars
            if text.eq_ignore_ascii_case("teleport") { return Some("teleport"); }
        }
        9 => {
            if text.eq_ignore_ascii_case("broadcast") { return Some("broadcast"); }
            if text.eq_ignore_ascii_case("variables") { return Some("variables"); }
        }
        _ => {}
    }
    None
}
// ---------------------------------------------------------------------------
// Construction & helpers
// ---------------------------------------------------------------------------

impl Lexer {
    /// Create a new lexer from source text.
    pub fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            indent_stack: Vec::new(),
            pending_dedents: 0,
            at_line_start: true,
            mode: LexMode::Normal,
            interp_depth: 0,
            string_buf: String::new(),
            has_interp: false,
            pending: None,
        }
    }

    /// Peek at the current character without consuming it.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Peek one character ahead.
    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    /// Advance one character and return it.
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    /// Advance past a newline (`\n`, `\r`, or `\r\n`) and return `true`.
    fn skip_newline(&mut self) -> bool {
        match self.peek() {
            Some('\n') => {
                self.pos += 1;
                true
            }
            Some('\r') => {
                self.pos += 1;
                if self.peek() == Some('\n') {
                    self.pos += 1;
                }
                true
            }
            _ => false,
        }
    }

    /// Skip the rest of the current line (including its newline terminator).
    fn skip_to_end_of_line(&mut self) {
        loop {
            match self.peek() {
                Some('\n') => {
                    self.pos += 1;
                    return;
                }
                Some('\r') => {
                    self.pos += 1;
                    if self.peek() == Some('\n') {
                        self.pos += 1;
                    }
                    return;
                }
                None => return,
                _ => { self.pos += 1; }
            }
        }
    }
}
// ---------------------------------------------------------------------------
// Main token dispatch
// ---------------------------------------------------------------------------

impl Lexer {
    /// Return the next token from the source.
    pub fn next_token(&mut self) -> SpannedToken {
        loop {
            if let Some(tok) = self.pending.take() {
                return tok;
            }
            if self.pending_dedents > 0 {
                self.pending_dedents -= 1;
                return SpannedToken {
                    token: Token::Dedent,
                    span: Span::new(self.pos, self.pos),
                };
            }

            if self.at_line_start && self.mode == LexMode::Normal {
                if let Some(tok) = self.handle_indentation() {
                    return tok;
                }
                // After skipping a blank line, check for EOF to avoid
                // infinite loop.
                if self.pos >= self.chars.len() {
                    if self.indent_stack.len() > 1 {
                        self.pending_dedents = self.indent_stack.len() - 1;
                        self.indent_stack.truncate(1);
                        return self.next_token();
                    }
                    return SpannedToken {
                        token: Token::Eof,
                        span: Span::new(self.pos, self.pos),
                    };
                }
                continue;
            }

            if self.pos >= self.chars.len() && self.mode == LexMode::Normal {
                if self.indent_stack.len() > 1 {
                    self.pending_dedents = self.indent_stack.len() - 1;
                    self.indent_stack.truncate(1);
                    return self.next_token();
                }
                return SpannedToken {
                    token: Token::Eof,
                    span: Span::new(self.pos, self.pos),
                };
            }

            break;
        }

        match self.mode {
            LexMode::Normal => self.next_token_normal(),
            LexMode::InString => self.next_token_in_string(),
            LexMode::InStringInterpolation => self.next_token_in_string_interp(),
        }
    }

    fn handle_indentation(&mut self) -> Option<SpannedToken> {
        let line_start = self.pos;
        let mut indent_width: usize = 0;
        let mut scan = self.pos;

        while scan < self.chars.len() {
            match self.chars[scan] {
                ' ' => { indent_width += 1; scan += 1; }
                '\t' => {
                    indent_width += 4 - (indent_width % 4);
                    scan += 1;
                }
                _ => break,
            }
        }

        let blank = scan >= self.chars.len()
            || self.chars[scan] == '\n'
            || self.chars[scan] == '\r'
            || self.chars[scan] == '#';

        if blank {
            self.pos = line_start;
            self.skip_to_end_of_line();
            return None;
        }

        self.pos = scan;
        self.at_line_start = false;

        if self.indent_stack.is_empty() {
            self.indent_stack.push(indent_width);
            return None;
        }

        let last = self.indent_stack[self.indent_stack.len() - 1];

        if indent_width > last {
            self.indent_stack.push(indent_width);
            Some(SpannedToken {
                token: Token::Indent,
                span: Span::new(line_start, self.pos),
            })
        } else if indent_width < last {
            while self.indent_stack.len() > 1 {
                let top = self.indent_stack[self.indent_stack.len() - 1];
                if top > indent_width {
                    self.indent_stack.pop();
                    self.pending_dedents += 1;
                } else {
                    break;
                }
            }
            None
        } else {
            None
        }
    }
}
// ---------------------------------------------------------------------------
// Normal mode tokeniser
// ---------------------------------------------------------------------------

impl Lexer {
    fn next_token_normal(&mut self) -> SpannedToken {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.advance();
            } else {
                break;
            }
        }

        let start = self.pos;

        if self.pos >= self.chars.len() {
            return self.eof_or_dedent();
        }

        let c = self.chars[self.pos];

        if c == '#' {
            self.skip_to_end_of_line();
            return self.next_token();
        }

        if c == '\n' || c == '\r' {
            self.skip_newline();
            self.at_line_start = true;
            return SpannedToken {
                token: Token::Newline,
                span: Span::new(start, self.pos),
            };
        }

        if c == '"' {
            self.pos += 1;
            self.mode = LexMode::InString;
            self.string_buf.clear();
            self.has_interp = false;
            return self.next_token_in_string();
        }

        if c == '{' {
            return self.parse_variable();
        }

        if c.is_ascii_digit()
            || ((c == '+' || c == '-')
                && self.peek_next().map_or(false, |n| n.is_ascii_digit()))
        {
            return self.parse_number(start);
        }

        if c.is_ascii_alphabetic() || c == '_' {
            return self.parse_word(start);
        }

        macro_rules! try_op {
            ($first:expr, $second:expr, $tok:ident, $len:expr) => {
                if c == $first && self.peek_next() == Some($second) {
                    self.pos += $len;
                    return SpannedToken {
                        token: Token::$tok,
                        span: Span::new(start, self.pos),
                    };
                }
            };
        }
        try_op!('=', '=', EqualEqual, 2);
        try_op!('!', '=', NotEqual, 2);
        try_op!('<', '=', LessEqual, 2);
        try_op!('>', '=', GreaterEqual, 2);
        try_op!('+', '=', PlusEqual, 2);
        try_op!('-', '=', MinusEqual, 2);
        try_op!('*', '=', StarEqual, 2);
        try_op!('/', '=', SlashEqual, 2);
        try_op!(':', '=', ColonEqual, 2);
        try_op!('-', '>', Arrow, 2);

        self.pos += 1;
        let token = match c {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '^' => Token::Caret,
            '~' => Token::Tilde,
            '<' => Token::Less,
            '>' => Token::Greater,
            '=' => Token::Equal,
            ':' => Token::Colon,
            '.' => Token::Dot,
            ',' => Token::Comma,
            ';' => Token::Semicolon,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            _ => return self.next_token(),
        };
        SpannedToken {
            token,
            span: Span::new(start, self.pos),
        }
    }

    fn parse_variable(&mut self) -> SpannedToken {
        let start = self.pos;
        self.pos += 1;
        let mut depth: u32 = 1;
        let mut content = String::new();

        while self.pos < self.chars.len() {
            // Handle %{ ... }% as a matched pair inside the variable.
            if self.chars[self.pos] == '%'
                && self.pos + 1 < self.chars.len()
                && self.chars[self.pos + 1] == '{'
            {
                content.push('%');
                content.push('{');
                self.pos += 2;
                // Consume until matching }%
                while self.pos + 1 < self.chars.len() {
                    if self.chars[self.pos] == '}' && self.chars[self.pos + 1] == '%' {
                        content.push('}');
                        content.push('%');
                        self.pos += 2;
                        break;
                    }
                    content.push(self.chars[self.pos]);
                    self.pos += 1;
                }
                continue;
            }

            let c = self.chars[self.pos];
            self.pos += 1;

            match c {
                '{' => {
                    depth += 1;
                    content.push('{');
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let parts: Vec<String> =
                            content.split("::").map(|s| s.to_string()).collect();
                        return SpannedToken {
                            token: Token::Variable(parts),
                            span: Span::new(start, self.pos),
                        };
                    }
                    content.push('}');
                }
                _ => { content.push(c); }
            }
        }

        SpannedToken {
            token: Token::Variable(vec![content]),
            span: Span::new(start, self.pos),
        }
    }

    fn parse_number(&mut self, start: usize) -> SpannedToken {
        if self.chars[start] == '+' || self.chars[start] == '-' {
            self.pos += 1;
        }
        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos < self.chars.len() && self.chars[self.pos] == '.' {
            self.pos += 1;
            while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        SpannedToken {
            token: Token::Number(text),
            span: Span::new(start, self.pos),
        }
    }

    fn parse_word(&mut self, start: usize) -> SpannedToken {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '\'' {
                self.advance();
            } else {
                break;
            }
        }

        let text: String = self.chars[start..self.pos].iter().collect();

        if text.eq_ignore_ascii_case("true") {
            return SpannedToken {
                token: Token::Boolean(true),
                span: Span::new(start, self.pos),
            };
        }
        if text.eq_ignore_ascii_case("false") {
            return SpannedToken {
                token: Token::Boolean(false),
                span: Span::new(start, self.pos),
            };
        }

        if let Some(canonical) = match_keyword(&text) {
            return SpannedToken {
                token: Token::Keyword(canonical),
                span: Span::new(start, self.pos),
            };
        }

        SpannedToken {
            token: Token::Identifier(text),
            span: Span::new(start, self.pos),
        }
    }

    fn eof_or_dedent(&mut self) -> SpannedToken {
        if self.indent_stack.len() > 1 {
            self.pending_dedents = self.indent_stack.len() - 1;
            self.indent_stack.truncate(1);
            return self.next_token();
        }
        SpannedToken {
            token: Token::Eof,
            span: Span::new(self.pos, self.pos),
        }
    }
}
// ---------------------------------------------------------------------------
// InString mode tokeniser
// ---------------------------------------------------------------------------

impl Lexer {
    fn next_token_in_string(&mut self) -> SpannedToken {
        loop {
            if self.pos >= self.chars.len() {
                self.mode = LexMode::Normal;
                if self.has_interp {
                    if self.string_buf.is_empty() {
                        return self.eof_or_dedent();
                    }
                    let text = std::mem::take(&mut self.string_buf);
                    let span = Span::new(self.pos - text.len(), self.pos);
                    return SpannedToken {
                        token: Token::StrText(text),
                        span,
                    };
                }
                if self.string_buf.is_empty() {
                    return self.eof_or_dedent();
                }
                let text = std::mem::take(&mut self.string_buf);
                let span = Span::new(self.pos - text.len(), self.pos);
                return SpannedToken {
                    token: Token::String(text),
                    span,
                };
            }

            let c = self.chars[self.pos];

            // Closing quote.
            if c == '"' {
                self.pos += 1;
                self.mode = LexMode::Normal;
                if self.has_interp {
                    if self.string_buf.is_empty() {
                        return self.next_token();
                    }
                    let text = std::mem::take(&mut self.string_buf);
                    let start = self.pos - text.len() - 1;
                    return SpannedToken {
                        token: Token::StrText(text),
                        span: Span::new(start, self.pos - 1),
                    };
                }
                let text = std::mem::take(&mut self.string_buf);
                let start = self.pos - text.len() - 1;
                return SpannedToken {
                    token: Token::String(text),
                    span: Span::new(start, self.pos - 1),
                };
            }

            // Interpolation start: %{
            if c == '%' && self.peek_next() == Some('{') {
                self.has_interp = true;
                if !self.string_buf.is_empty() {
                    let text = std::mem::take(&mut self.string_buf);
                    let span = Span::new(self.pos - text.len(), self.pos);
                    self.pos += 2;
                    self.mode = LexMode::InStringInterpolation;
                    self.interp_depth = 1;
                    self.pending = Some(SpannedToken {
                        token: Token::StrInterpolationStart,
                        span: Span::new(self.pos - 2, self.pos),
                    });
                    return SpannedToken {
                        token: Token::StrText(text),
                        span,
                    };
                }
                self.pos += 2;
                self.mode = LexMode::InStringInterpolation;
                self.interp_depth = 1;
                return SpannedToken {
                    token: Token::StrInterpolationStart,
                    span: Span::new(self.pos - 2, self.pos),
                };
            }

            // MiniMessage tag: <alpha or </alpha
            if c == '<' {
                let next = self.peek_next();
                if let Some(n) = next {
                    if n.is_ascii_alphabetic() || n == '/' {
                        if !self.string_buf.is_empty() {
                            let text = std::mem::take(&mut self.string_buf);
                            let span = Span::new(self.pos - text.len(), self.pos);
                            return SpannedToken {
                                token: Token::StrText(text),
                                span,
                            };
                        }
                        if let Some(tok) = self.parse_minimessage_tag() {
                            return tok;
                        }
                    }
                }
            }

            // Regular character.
            self.string_buf.push(c);
            self.pos += 1;
        }
    }

    /// Parse a MiniMessage tag `<tag>` or `</tag>` at the current position.
    /// Assumes `<` is at `self.pos` and the following char is valid.
    /// Sets `has_interp = true` so that text after the tag is StrText.
    fn parse_minimessage_tag(&mut self) -> Option<SpannedToken> {
        let start = self.pos;
        let is_close = self.peek_next() == Some('/');
        let content_start = if is_close { start + 2 } else { start + 1 };

        if content_start >= self.chars.len() {
            return None;
        }

        let mut tag_end = content_start;
        while tag_end < self.chars.len() {
            let c = self.chars[tag_end];
            if c == '>' || c == '/' || c == ' ' || c == '\t' {
                break;
            }
            if !c.is_ascii_alphanumeric() && c != '_' && c != '-' {
                return None;
            }
            tag_end += 1;
        }

        if tag_end >= self.chars.len() || self.chars[tag_end] != '>' {
            return None;
        }
        tag_end += 1;

        if tag_end - content_start == 0 {
            return None;
        }

        let tag_name: String = self.chars[content_start..tag_end - 1].iter().collect();
        self.pos = tag_end;
        self.has_interp = true;

        if is_close {
            Some(SpannedToken {
                token: Token::MiniMessageClose(tag_name),
                span: Span::new(start, self.pos),
            })
        } else {
            Some(SpannedToken {
                token: Token::MiniMessageOpen(tag_name),
                span: Span::new(start, self.pos),
            })
        }
    }
}
// ---------------------------------------------------------------------------
// InStringInterpolation mode tokeniser
// ---------------------------------------------------------------------------

impl Lexer {
    fn next_token_in_string_interp(&mut self) -> SpannedToken {
        loop {
            if self.pos >= self.chars.len() {
                self.mode = LexMode::Normal;
                return self.eof_or_dedent();
            }

            let start = self.pos;
            let c = self.chars[self.pos];

            if c == ' ' || c == '\t' {
                self.pos += 1;
                continue;
            }

            if c == '}' {
                if self.interp_depth <= 1 && self.peek_next() == Some('%') {
                    self.pos += 2;
                    self.mode = LexMode::InString;
                    return SpannedToken {
                        token: Token::StrInterpolationEnd,
                        span: Span::new(start, self.pos),
                    };
                }
                self.pos += 1;
                if self.interp_depth > 0 {
                    self.interp_depth -= 1;
                }
                return SpannedToken {
                    token: Token::RBrace,
                    span: Span::new(start, self.pos),
                };
            }

            if c == '{' {
                self.pos += 1;
                self.interp_depth += 1;
                return SpannedToken {
                    token: Token::LBrace,
                    span: Span::new(start, self.pos),
                };
            }

            if c == '#' {
                self.skip_to_end_of_line();
                continue;
            }

            if c.is_ascii_digit()
                || ((c == '+' || c == '-')
                    && self.peek_next().map_or(false, |n| n.is_ascii_digit()))
            {
                return self.parse_number(start);
            }

            if c.is_ascii_alphabetic() || c == '_' {
                return self.parse_word(start);
            }

            macro_rules! try_op {
                ($first:expr, $second:expr, $tok:ident, $len:expr) => {
                    if c == $first && self.peek_next() == Some($second) {
                        self.pos += $len;
                        return SpannedToken {
                            token: Token::$tok,
                            span: Span::new(start, self.pos),
                        };
                    }
                };
            }
            try_op!('=', '=', EqualEqual, 2);
            try_op!('!', '=', NotEqual, 2);
            try_op!('<', '=', LessEqual, 2);
            try_op!('>', '=', GreaterEqual, 2);
            try_op!('+', '=', PlusEqual, 2);
            try_op!('-', '=', MinusEqual, 2);
            try_op!('*', '=', StarEqual, 2);
            try_op!('/', '=', SlashEqual, 2);
            try_op!(':', '=', ColonEqual, 2);
            try_op!('-', '>', Arrow, 2);

            self.pos += 1;
            let token = match c {
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                '%' => Token::Percent,
                '^' => Token::Caret,
                '~' => Token::Tilde,
                '<' => Token::Less,
                '>' => Token::Greater,
                '=' => Token::Equal,
                ':' => Token::Colon,
                '.' => Token::Dot,
                ',' => Token::Comma,
                ';' => Token::Semicolon,
                '(' => Token::LParen,
                ')' => Token::RParen,
                '[' => Token::LBracket,
                ']' => Token::RBracket,
                _ => continue,
            };
            return SpannedToken {
                token,
                span: Span::new(start, self.pos),
            };
        }
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let st = lexer.next_token();
            let is_eof = matches!(st.token, Token::Eof);
            tokens.push(st.token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn spanned(source: &str) -> Vec<SpannedToken> {
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

    // 1. Empty source
    #[test]
    fn test_empty_source() {
        let toks = tokenize("");
        assert_eq!(toks, vec![Token::Eof]);
    }

    // 2. Keyword tokens
    #[test]
    fn test_keyword_tokens() {
        let cases = [
            "on", "if", "else", "loop", "while", "for", "set", "delete",
            "clear", "add", "remove", "give", "function", "command",
            "trigger", "options", "execute", "return", "stop", "exit",
            "continue", "cancel", "wait", "broadcast", "send", "make",
            "spawn", "teleport", "heal", "damage", "play", "run", "with",
            "at", "to", "from", "into", "of", "in", "named", "where", "by",
            "as", "and", "or", "not", "is", "isn't", "every", "after",
            "before", "using", "aliases", "variables", "example", "do",
        ];
        for kw in &cases {
            let toks = tokenize(kw);
            assert!(
                matches!(toks.first(), Some(Token::Keyword(s)) if *s == *kw),
                "expected keyword {kw:?}, got {toks:?}",
            );
        }
    }

    #[test]
    fn test_keyword_case_insensitive() {
        let toks = tokenize("IF");
        assert_eq!(toks, vec![Token::Keyword("if"), Token::Eof]);
    }

    #[test]
    fn test_keyword_mixed_case() {
        let toks = tokenize("Broadcast");
        assert_eq!(toks, vec![Token::Keyword("broadcast"), Token::Eof]);
    }

    // 3. Number tokens
    #[test]
    fn test_number_integer() {
        let toks = tokenize("42");
        assert_eq!(toks, vec![Token::Number("42".into()), Token::Eof]);
    }

    #[test]
    fn test_number_float() {
        let toks = tokenize("3.14");
        assert_eq!(toks, vec![Token::Number("3.14".into()), Token::Eof]);
    }

    #[test]
    fn test_number_negative() {
        let toks = tokenize("-5");
        assert_eq!(
            toks,
            vec![Token::Number("-5".into()), Token::Eof]
        );
    }

    // 4. Simple string
    #[test]
    fn test_string_simple() {
        let toks = tokenize(r#""hello""#);
        assert_eq!(toks, vec![Token::String("hello".into()), Token::Eof]);
    }

    #[test]
    fn test_string_empty() {
        let toks = tokenize(r#""""#);
        assert_eq!(toks, vec![Token::String("".into()), Token::Eof]);
    }

    // 5. Simple variables
    #[test]
    fn test_variable_simple() {
        let toks = tokenize("{_var}");
        assert_eq!(
            toks,
            vec![Token::Variable(vec!["_var".into()]), Token::Eof]
        );
    }

    #[test]
    fn test_variable_list() {
        let toks = tokenize("{list::*}");
        assert_eq!(
            toks,
            vec![
                Token::Variable(vec!["list".into(), "*".into()]),
                Token::Eof,
            ]
        );
    }

    // 6. Nested variable with interpolation
    #[test]
    fn test_variable_nested() {
        let toks = tokenize("{_::%{x}%}");
        assert_eq!(
            toks,
            vec![
                Token::Variable(vec!["_".into(), "%{x}%".into()]),
                Token::Eof,
            ]
        );
    }

    // 7. Indent / dedent
    #[test]
    fn test_indent_dedent() {
        let source = "if true is true:\n    broadcast \"hi\"\nbroadcast \"bye\"";
        let toks = tokenize(source);
        assert!(toks.contains(&Token::Indent), "expected Indent token");
        assert!(toks.contains(&Token::Dedent), "expected Dedent token");
    }

    // 8. Multi-dedent
    #[test]
    fn test_multi_dedent() {
        let source =
            "loop 5 times:\n    if true:\n        broadcast \"hi\"\nbroadcast \"bye\"";
        let toks: Vec<_> = tokenize(source)
            .into_iter()
            .filter(|t| matches!(t, Token::Indent | Token::Dedent))
            .collect();
        assert_eq!(toks.len(), 4);
        assert_eq!(toks[0], Token::Indent);
        assert_eq!(toks[1], Token::Indent);
        assert_eq!(toks[2], Token::Dedent);
        assert_eq!(toks[3], Token::Dedent);
    }

    // 9. Operators
    #[test]
    fn test_arithmetic_operators() {
        let toks = tokenize("+ - * / % ^ ~");
        assert_eq!(
            toks,
            vec![
                Token::Plus, Token::Minus, Token::Star, Token::Slash,
                Token::Percent, Token::Caret, Token::Tilde, Token::Eof,
            ]
        );
    }

    #[test]
    fn test_comparison_operators() {
        let toks = tokenize("== != <= >= < >");
        assert_eq!(
            toks,
            vec![
                Token::EqualEqual, Token::NotEqual, Token::LessEqual,
                Token::GreaterEqual, Token::Less, Token::Greater, Token::Eof,
            ]
        );
    }

    #[test]
    fn test_assignment_operators() {
        let toks = tokenize("= := += -= *= /=");
        assert_eq!(
            toks,
            vec![
                Token::Equal, Token::ColonEqual, Token::PlusEqual,
                Token::MinusEqual, Token::StarEqual, Token::SlashEqual,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_dot_comma_arrow() {
        let toks = tokenize(". , ->");
        assert_eq!(
            toks,
            vec![Token::Dot, Token::Comma, Token::Arrow, Token::Eof]
        );
    }

    #[test]
    fn test_parens_brackets() {
        let toks = tokenize("( ) [ ]");
        assert_eq!(
            toks,
            vec![
                Token::LParen, Token::RParen, Token::LBracket, Token::RBracket,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_colon() {
        let toks = tokenize(":");
        assert_eq!(toks, vec![Token::Colon, Token::Eof]);
    }

    // 10. String interpolation
    #[test]
    fn test_string_interpolation() {
        let source = r#""hello %{name}% world""#;
        let toks = tokenize(source);
        assert_eq!(
            toks,
            vec![
                Token::StrText("hello ".into()),
                Token::StrInterpolationStart,
                Token::Identifier("name".into()),
                Token::StrInterpolationEnd,
                Token::StrText(" world".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_string_interpolation_trailing() {
        let source = r#""%{name}% world""#;
        let toks = tokenize(source);
        assert_eq!(
            toks,
            vec![
                Token::StrInterpolationStart,
                Token::Identifier("name".into()),
                Token::StrInterpolationEnd,
                Token::StrText(" world".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_string_interpolation_leading() {
        let source = r#""hello %{name}%""#;
        let toks = tokenize(source);
        assert_eq!(
            toks,
            vec![
                Token::StrText("hello ".into()),
                Token::StrInterpolationStart,
                Token::Identifier("name".into()),
                Token::StrInterpolationEnd,
                Token::Eof,
            ]
        );
    }

    // 11. MiniMessage tags
    #[test]
    fn test_minimessage_open_tag() {
        let source = r#""<red>text""#;
        let toks = tokenize(source);
        assert_eq!(
            toks,
            vec![
                Token::MiniMessageOpen("red".into()),
                Token::StrText("text".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_minimessage_close_tag() {
        let source = r#""</red>text""#;
        let toks = tokenize(source);
        assert_eq!(
            toks,
            vec![
                Token::MiniMessageClose("red".into()),
                Token::StrText("text".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_minimessage_both() {
        let source = r#""<red>text</red>""#;
        let toks = tokenize(source);
        assert_eq!(
            toks,
            vec![
                Token::MiniMessageOpen("red".into()),
                Token::StrText("text".into()),
                Token::MiniMessageClose("red".into()),
                Token::Eof,
            ]
        );
    }

    // 12. Comment skipped
    #[test]
    fn test_comment_skipped() {
        let toks = tokenize("# this is a comment");
        assert_eq!(toks, vec![Token::Eof]);
    }

    // Additional edge-case tests
    #[test]
    fn test_boolean_true() {
        let toks = tokenize("true");
        assert_eq!(toks, vec![Token::Boolean(true), Token::Eof]);
    }

    #[test]
    fn test_boolean_false() {
        let toks = tokenize("false");
        assert_eq!(toks, vec![Token::Boolean(false), Token::Eof]);
    }

    #[test]
    fn test_identifier_with_dash() {
        let toks = tokenize("arg-1");
        assert_eq!(
            toks,
            vec![Token::Identifier("arg-1".into()), Token::Eof]
        );
    }

    #[test]
    fn test_identifier_with_underscore() {
        let toks = tokenize("my_var");
        assert_eq!(
            toks,
            vec![Token::Identifier("my_var".into()), Token::Eof]
        );
    }

    #[test]
    fn test_newline_between_tokens() {
        let toks = tokenize("set\n{_x}");
        assert!(toks.contains(&Token::Newline));
        assert!(toks.contains(&Token::Variable(vec!["_x".into()])));
        assert!(toks.contains(&Token::Keyword("set")));
    }

    #[test]
    fn test_only_whitespace() {
        let toks = tokenize("   \n  \n");
        // After handling blank lines, should get Eof
        assert_eq!(toks, vec![Token::Eof]);
    }

    #[test]
    fn test_only_comments() {
        let toks = tokenize("# a\n# b\n# c\n");
        assert_eq!(toks, vec![Token::Eof]);
    }

    #[test]
    fn test_inline_comment() {
        let toks = tokenize("set # not a set");
        assert_eq!(toks, vec![Token::Keyword("set"), Token::Eof]);
    }

    #[test]
    fn test_mixed_tabs_spaces() {
        let toks = tokenize("if true:\n\tbroadcast \"hi\"\nbroadcast \"bye\"");
        assert!(toks.contains(&Token::Indent));
        assert!(toks.contains(&Token::Dedent));
    }

    #[test]
    fn test_eof_dedents() {
        let source = "if true:\n    broadcast \"hi\"";
        let toks = tokenize(source);
        let dedents: usize = toks.iter().filter(|t| **t == Token::Dedent).count();
        assert_eq!(dedents, 1);
    }

    #[test]
    fn test_spanned_token_positions() {
        let sts = spanned("if");
        assert_eq!(sts.len(), 2);
        assert_eq!(sts[0].span.start, 0);
        assert_eq!(sts[0].span.end, 2);
    }

    #[test]
    fn test_string_interpolation_in_variable() {
        let toks = tokenize("{_::%{x}%}");
        assert_eq!(
            toks,
            vec![
                Token::Variable(vec!["_".into(), "%{x}%".into()]),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_minimessage_with_interpolation() {
        let source = r#""<green>%{name}%</green>""#;
        let toks = tokenize(source);
        assert!(toks.contains(&Token::MiniMessageOpen("green".into())));
        assert!(toks.contains(&Token::MiniMessageClose("green".into())));
        assert!(toks.contains(&Token::StrInterpolationStart));
        assert!(toks.contains(&Token::StrInterpolationEnd));
        assert!(toks.contains(&Token::Identifier("name".into())));
    }

    #[test]
    fn test_isnt_keyword() {
        let toks = tokenize("isn't");
        assert_eq!(toks, vec![Token::Keyword("isn't"), Token::Eof]);
    }

    #[test]
    fn test_spanned_consistency() {
        let source = "42";
        let sts = spanned(source);
        assert_eq!(sts[0].span, Span::new(0, 2));
        if let Token::Number(ref n) = sts[0].token {
            assert_eq!(n, "42");
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn test_multiple_operators_no_space() {
        let toks = tokenize("+ -");
        assert_eq!(
            toks,
            vec![Token::Plus, Token::Minus, Token::Eof]
        );
    }

    #[test]
    fn test_keyword_followed_by_identifier() {
        let toks = tokenize("if true");
        assert_eq!(
            toks,
            vec![Token::Keyword("if"), Token::Boolean(true), Token::Eof]
        );
    }

    #[test]
    fn test_indented_comment_line() {
        let source = "set {_x} to 1\n    # indented comment\n    broadcast {_x}";
        let toks = tokenize(source);
        let idx_broadcast = toks.iter().position(|t| matches!(t, Token::Keyword(k) if *k == "broadcast"));
        let idx_indent = toks.iter().position(|t| *t == Token::Indent);
        assert!(idx_indent.is_some());
        assert!(idx_broadcast.is_some());
        assert!(idx_indent.unwrap() < idx_broadcast.unwrap());
    }

    #[test]
    fn test_blank_line_between() {
        let source = "if true:\n    broadcast \"hi\"\n\n    broadcast \"bye\"";
        let toks = tokenize(source);
        let keywords: Vec<_> = toks
            .iter()
            .filter(|t| matches!(t, Token::Keyword(k) if *k == "broadcast"))
            .collect();
        assert_eq!(keywords.len(), 2);
        let indents: Vec<_> = toks.iter().filter(|t| **t == Token::Indent).collect();
        assert_eq!(indents.len(), 1);
    }

    #[test]
    fn test_yes_no_keywords() {
        let toks = tokenize("yes no");
        assert_eq!(
            toks,
            vec![Token::Keyword("yes"), Token::Keyword("no"), Token::Eof]
        );
    }
}
