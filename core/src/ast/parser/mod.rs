// The parser core: cursor state and the token-level helpers every rule uses.
// `stmt`/`expr` are children rather than siblings so they reach `Parser`'s
// private fields and these helpers without any of it being widened.
mod expr;
mod stmt;

use crate::ast::Ast;
use crate::ast::types::ParseError;
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    at_top_level: bool,
    in_loop: bool,
    depth: usize,
    // Caps recursive-descent nesting so malformed/malicious input can't
    // overflow the real stack while parsing - see `Config::max_parse_depth`
    // for the stack-safety rationale behind its default value.
    max_depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, max_depth: usize) -> Self {
        Parser {
            tokens,
            pos: 0,
            at_top_level: true,
            in_loop: false,
            depth: 0,
            max_depth,
        }
    }

    pub fn parse(mut self) -> Result<Ast, ParseError> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            nodes.push(self.statement()?);
        }
        Ok(Ast::from_nodes(nodes))
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if !self.is_at_end() {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        !self.is_at_end() && &self.peek().kind == kind
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, context: &str) -> Result<Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!("expected {} {}", kind, context)))
        }
    }

    fn expect_ident(&mut self, context: &str) -> Result<String, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error(&format!("expected identifier {}", context))),
        }
    }

    fn error(&self, message: &str) -> ParseError {
        let tok = self.peek();
        ParseError {
            message: message.to_string(),
            line: tok.line,
            col: tok.col,
        }
    }

    // Call at the top of every recursive-descent cycle; caller decrements
    // `depth` back down once its recursive work returns.
    fn enter_nesting(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(self.error("expression or block nested too deeply"));
        }
        Ok(())
    }
}
