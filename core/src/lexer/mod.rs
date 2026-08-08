mod types;
pub use types::{LexError, Token, TokenKind};

use std::collections::HashSet;

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    disabled_keywords: HashSet<String>,
}

impl Lexer {
    pub fn new(source: &str, disabled_keywords: &[String]) -> Self {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            disabled_keywords: disabled_keywords.iter().cloned().collect(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn matches(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    // line comment - bonus, since a real language usually needs this
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let start_line = self.line;
                    let start_col = self.col;
                    self.advance(); // consume '/'
                    self.advance(); // consume '*'
                    loop {
                        match self.peek() {
                            None => {
                                return Err(LexError {
                                    message: "unterminated block comment".to_string(),
                                    line: start_line,
                                    col: start_col,
                                });
                            }
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.advance();
                                self.advance();
                                break;
                            }
                            Some(_) => {
                                self.advance();
                            }
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn scan_string(&mut self) -> Result<TokenKind, LexError> {
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // consume opening "
        let mut value = String::new();

        loop {
            match self.peek() {
                None => {
                    return Err(LexError {
                        message: "unterminated string literal".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('"') => value.push('"'),
                        Some('\\') => value.push('\\'),
                        Some(other) => {
                            // unknown escape - pass through leniently rather than erroring
                            value.push('\\');
                            value.push(other);
                        }
                        None => {
                            return Err(LexError {
                                message: "unterminated string literal".to_string(),
                                line: start_line,
                                col: start_col,
                            });
                        }
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.advance();
                }
            }
        }

        Ok(TokenKind::Str(value))
    }

    fn scan_number(&mut self) -> Result<TokenKind, LexError> {
        let start_line = self.line;
        let start_col = self.col;
        let mut text = String::new();
        let mut is_float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }
        // optional fractional part - only consume '.' if followed by a digit,
        // so "some_object.property" doesn't get mangled near numbers
        if self.peek() == Some('.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
            is_float = true;
            text.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        if is_float {
            match text.parse::<f64>() {
                Ok(v) if v.is_finite() => Ok(TokenKind::Float(v)),
                _ => Err(LexError {
                    message: format!("float literal '{}' is out of range", text),
                    line: start_line,
                    col: start_col,
                }),
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(TokenKind::Int(v)),
                Err(_) => Err(LexError {
                    message: format!("integer literal '{}' is out of range", text),
                    line: start_line,
                    col: start_col,
                }),
            }
        }
    }

    fn scan_identifier(&mut self) -> Result<TokenKind, LexError> {
        let start_line = self.line;
        let start_col = self.col;
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }
        let keyword = match text.as_str() {
            "var" => Some(TokenKind::Var),
            "if" => Some(TokenKind::If),
            "else" => Some(TokenKind::Else),
            "true" => Some(TokenKind::True),
            "false" => Some(TokenKind::False),
            "func" => Some(TokenKind::Func),
            "return" => Some(TokenKind::Return),
            "null" => Some(TokenKind::Null),
            "while" => Some(TokenKind::While),
            "for" => Some(TokenKind::For),
            "break" => Some(TokenKind::Break),
            "continue" => Some(TokenKind::Continue),
            "match" => Some(TokenKind::Match),
            "case" => Some(TokenKind::Case),
            "default" => Some(TokenKind::Default),
            "in" => Some(TokenKind::In),
            "try" => Some(TokenKind::Try),
            "catch" => Some(TokenKind::Catch),
            "throw" => Some(TokenKind::Throw),
            "exit" => Some(TokenKind::Exit),
            _ => None,
        };

        // disabled_keywords only ever gates language keywords, never plain
        // identifiers - otherwise disabling "add" would also block a user's
        // own function/variable named "add", which isn't a keyword at all.
        if let Some(kind) = keyword {
            if self.disabled_keywords.contains(&text) {
                return Err(LexError {
                    message: format!("keyword '{}' is disabled", text),
                    line: start_line,
                    col: start_col,
                });
            }
            return Ok(kind);
        }

        Ok(TokenKind::Ident(text))
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments()?;

        let line = self.line;
        let col = self.col;

        let c = match self.peek() {
            None => {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    line,
                    col,
                });
            }
            Some(c) => c,
        };

        let kind = match c {
            '"' => self.scan_string()?,
            c if c.is_ascii_digit() => self.scan_number()?,
            c if c.is_alphabetic() || c == '_' => self.scan_identifier()?,
            ';' => {
                self.advance();
                TokenKind::Semicolon
            }
            '(' => {
                self.advance();
                TokenKind::LParen
            }
            ')' => {
                self.advance();
                TokenKind::RParen
            }
            '{' => {
                self.advance();
                TokenKind::LBrace
            }
            '}' => {
                self.advance();
                TokenKind::RBrace
            }
            '[' => {
                self.advance();
                TokenKind::LBracket
            }
            ']' => {
                self.advance();
                TokenKind::RBracket
            }
            '.' => {
                self.advance();
                TokenKind::Dot
            }
            ',' => {
                self.advance();
                TokenKind::Comma
            }
            ':' => {
                self.advance();
                TokenKind::Colon
            }
            '+' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::PlusEq
                } else if self.matches('+') {
                    TokenKind::PlusPlus
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::MinusEq
                } else if self.matches('-') {
                    TokenKind::MinusMinus
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            '=' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::Eq
                } else {
                    TokenKind::Assign
                }
            }
            '!' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::NotEq
                } else {
                    TokenKind::Not
                }
            }
            '<' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                self.advance();
                if self.matches('=') {
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                self.advance();
                if self.matches('&') {
                    TokenKind::AndAnd
                } else {
                    return Err(LexError {
                        message: "unexpected character '&' (did you mean '&&'?)".to_string(),
                        line,
                        col,
                    });
                }
            }
            '|' => {
                self.advance();
                if self.matches('|') {
                    TokenKind::OrOr
                } else {
                    return Err(LexError {
                        message: "unexpected character '|' (did you mean '||'?)".to_string(),
                        line,
                        col,
                    });
                }
            }
            other => {
                // `{:?}` (not `{}`) so a raw control character in the source
                // (e.g. a terminal escape byte typed outside a string
                // literal) shows up as a readable escape like '\u{1b}'
                // instead of being echoed to the host's terminal as-is.
                return Err(LexError {
                    message: format!("unexpected character {:?}", other),
                    line,
                    col,
                });
            }
        };

        Ok(Token { kind, line, col })
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }
}
