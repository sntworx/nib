use std::fmt;

use crate::ast::types::{
    AstNode, AstNodeKind, BinaryOp, Expr, ForStmt, FuncDecl, IfStmt, Literal, UnaryOp, VarAssign, WhileStmt,
};
use crate::ast::Ast;
use crate::lexer::{Token, TokenKind};

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Parse error at {}:{}: {}", self.line, self.col, self.message)
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    // Only true at the outermost statement list - false the instant the
    // parser enters any `{ ... }` block, whether from `if`/`while`/`for`/
    // `func` or a bare block statement. `func` declarations check this
    // directly, so they're rejected at any nesting depth, not just inside
    // another `func` or a loop specifically.
    at_top_level: bool,
    in_loop: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, at_top_level: true, in_loop: false }
    }

    pub fn parse(mut self) -> Result<Ast, ParseError> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            nodes.push(self.statement()?);
        }
        Ok(Ast::from_nodes(nodes))
    }

    // --- token helpers ---

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

    // --- statements ---

    fn statement(&mut self) -> Result<AstNode, ParseError> {
        let line = self.peek().line;
        let col = self.peek().col;

        let kind = if self.match_kind(&TokenKind::Var) {
            AstNodeKind::VarAssign(self.var_assign_stmt()?)
        } else if self.check(&TokenKind::If) {
            AstNodeKind::If(self.if_stmt()?)
        } else if self.check(&TokenKind::LBrace) {
            AstNodeKind::Block(self.block()?)
        } else if self.check(&TokenKind::Func) {
            AstNodeKind::FuncDecl(self.func_decl_stmt()?)
        } else if self.match_kind(&TokenKind::Return) {
            AstNodeKind::Return(self.return_stmt()?)
        } else if self.check(&TokenKind::While) {
            AstNodeKind::While(self.while_stmt()?)
        } else if self.check(&TokenKind::For) {
            AstNodeKind::For(self.for_stmt()?)
        } else if self.check(&TokenKind::Break) {
            self.break_stmt()?;
            AstNodeKind::Break
        } else if self.check(&TokenKind::Continue) {
            self.continue_stmt()?;
            AstNodeKind::Continue
        } else {
            AstNodeKind::ExprStmt(self.expr_stmt()?)
        };

        Ok(AstNode { kind, line, col })
    }

    fn func_decl_stmt(&mut self) -> Result<FuncDecl, ParseError> {
        if !self.at_top_level {
            return Err(self.error("function declarations are only allowed at the top level"));
        }
        self.expect(&TokenKind::Func, "")?;
        let name = self.expect_ident("after 'func'")?;
        self.expect(&TokenKind::LParen, "after function name")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                params.push(self.expect_ident("in parameter list")?);
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "after parameters")?;

        // `block()` itself drops `at_top_level` for the body, so a `func`
        // can never be declared inside another `func`'s body either
        let body = self.block()?;

        Ok(FuncDecl { name, params, body })
    }

    fn break_stmt(&mut self) -> Result<(), ParseError> {
        if !self.in_loop {
            return Err(self.error("'break' outside of loop"));
        }
        self.expect(&TokenKind::Break, "")?;
        self.expect(&TokenKind::Semicolon, "after 'break'")?;
        Ok(())
    }

    fn continue_stmt(&mut self) -> Result<(), ParseError> {
        if !self.in_loop {
            return Err(self.error("'continue' outside of loop"));
        }
        self.expect(&TokenKind::Continue, "")?;
        self.expect(&TokenKind::Semicolon, "after 'continue'")?;
        Ok(())
    }

    fn return_stmt(&mut self) -> Result<Option<Expr>, ParseError> {
        if self.match_kind(&TokenKind::Semicolon) {
            return Ok(None);
        }
        let value = self.expression()?;
        self.expect(&TokenKind::Semicolon, "after return value")?;
        Ok(Some(value))
    }

    fn var_assign_stmt(&mut self) -> Result<VarAssign, ParseError> {
        let name = self.expect_ident("after 'var'")?;
        self.expect(&TokenKind::Assign, "after variable name")?;
        let value = self.expression()?;
        self.expect(&TokenKind::Semicolon, "after variable declaration")?;
        Ok(VarAssign { name, value })
    }

    fn if_stmt(&mut self) -> Result<IfStmt, ParseError> {
        self.expect(&TokenKind::If, "")?;
        // condition is a bare expression; `(cond)` still works since parens
        // are already valid as a grouping expression
        let condition = self.expression()?;
        let then_branch = self.block()?;
        let else_branch = if self.match_kind(&TokenKind::Else) {
            if self.check(&TokenKind::If) {
                let line = self.peek().line;
                let col = self.peek().col;
                let nested = self.if_stmt()?;
                Some(vec![AstNode { kind: AstNodeKind::If(nested), line, col }])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(IfStmt { condition, then_branch, else_branch })
    }

    fn while_stmt(&mut self) -> Result<WhileStmt, ParseError> {
        self.expect(&TokenKind::While, "")?;
        // condition is a bare expression, same as `if` - `(cond)` still works
        // since parens are already valid as a grouping expression
        let condition = self.expression()?;

        let saved_in_loop = self.in_loop;
        self.in_loop = true;
        let body = self.block();
        self.in_loop = saved_in_loop;

        Ok(WhileStmt { condition, body: body? })
    }

    fn for_stmt(&mut self) -> Result<ForStmt, ParseError> {
        self.expect(&TokenKind::For, "")?;
        self.expect(&TokenKind::LParen, "after 'for'")?;

        let init = if self.match_kind(&TokenKind::Semicolon) {
            None
        } else {
            let line = self.peek().line;
            let col = self.peek().col;
            let kind = if self.match_kind(&TokenKind::Var) {
                AstNodeKind::VarAssign(self.var_assign_stmt()?)
            } else {
                AstNodeKind::ExprStmt(self.expr_stmt()?)
            };
            Some(Box::new(AstNode { kind, line, col }))
        };

        let condition = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(&TokenKind::Semicolon, "after for-loop condition")?;

        let post = if self.check(&TokenKind::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(&TokenKind::RParen, "after for-loop clauses")?;

        let saved_in_loop = self.in_loop;
        self.in_loop = true;
        let body = self.block();
        self.in_loop = saved_in_loop;

        Ok(ForStmt { init, condition, post, body: body? })
    }

    fn block(&mut self) -> Result<Vec<AstNode>, ParseError> {
        self.expect(&TokenKind::LBrace, "to start block")?;

        let saved_at_top_level = self.at_top_level;
        self.at_top_level = false;
        let nodes = self.block_statements();
        self.at_top_level = saved_at_top_level;
        let nodes = nodes?;

        self.expect(&TokenKind::RBrace, "to close block")?;
        Ok(nodes)
    }

    fn block_statements(&mut self) -> Result<Vec<AstNode>, ParseError> {
        let mut nodes = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            nodes.push(self.statement()?);
        }
        Ok(nodes)
    }

    fn expr_stmt(&mut self) -> Result<Expr, ParseError> {
        let expr = self.expression()?;
        self.expect(&TokenKind::Semicolon, "after expression")?;
        Ok(expr)
    }

    // --- expressions (precedence climbing, lowest to highest) ---

    fn expression(&mut self) -> Result<Expr, ParseError> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, ParseError> {
        let expr = self.or()?;

        // `x++`/`x--` desugar to `x = x + 1`/`x = x - 1`, same trick as
        // compound assignment below. Since they take no right-hand operand,
        // this only reaches as far as a bare identifier - not embeddable
        // mid-expression like `1 + x++`, same limitation compound assignment
        // already has.
        if self.match_kind(&TokenKind::PlusPlus) {
            return self.incr_decr(expr, BinaryOp::Add);
        }
        if self.match_kind(&TokenKind::MinusMinus) {
            return self.incr_decr(expr, BinaryOp::Sub);
        }

        // `x += value` desugars to `x = x + value` (and so on for -=/*=//=), so
        // no separate AST/interpreter support is needed for compound assignment
        let compound_op = if self.match_kind(&TokenKind::PlusEq) {
            Some(BinaryOp::Add)
        } else if self.match_kind(&TokenKind::MinusEq) {
            Some(BinaryOp::Sub)
        } else if self.match_kind(&TokenKind::StarEq) {
            Some(BinaryOp::Mul)
        } else if self.match_kind(&TokenKind::SlashEq) {
            Some(BinaryOp::Div)
        } else if self.match_kind(&TokenKind::Assign) {
            None
        } else {
            return Ok(expr);
        };

        let value = self.assignment()?; // right-associative
        match expr {
            Expr::Ident(name) => {
                let value = match compound_op {
                    Some(op) => {
                        Expr::Binary { op, left: Box::new(Expr::Ident(name.clone())), right: Box::new(value) }
                    }
                    None => value,
                };
                Ok(Expr::Assign { name, value: Box::new(value) })
            }
            // unlike the Ident case above, the compound op isn't desugared
            // here into a Binary - the interpreter applies it directly so
            // `object`/`index` only get evaluated once (they may have side
            // effects, e.g. `arr[i()] += 1`)
            Expr::Index { object, index } => {
                Ok(Expr::IndexAssign { object, index, op: compound_op, value: Box::new(value) })
            }
            _ => Err(self.error("invalid assignment target")),
        }
    }

    fn incr_decr(&mut self, expr: Expr, op: BinaryOp) -> Result<Expr, ParseError> {
        match expr {
            Expr::Ident(name) => Ok(Expr::Assign {
                name: name.clone(),
                value: Box::new(Expr::Binary {
                    op,
                    left: Box::new(Expr::Ident(name)),
                    right: Box::new(Expr::Literal(Literal::Int(1))),
                }),
            }),
            Expr::Index { object, index } => Ok(Expr::IndexAssign {
                object,
                index,
                op: Some(op),
                value: Box::new(Expr::Literal(Literal::Int(1))),
            }),
            _ => Err(self.error("invalid increment/decrement target")),
        }
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.and()?;
        while self.match_kind(&TokenKind::OrOr) {
            let right = self.and()?;
            expr = Expr::Binary { op: BinaryOp::Or, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.equality()?;
        while self.match_kind(&TokenKind::AndAnd) {
            let right = self.equality()?;
            expr = Expr::Binary { op: BinaryOp::And, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.comparison()?;
        loop {
            let op = if self.match_kind(&TokenKind::Eq) {
                BinaryOp::Eq
            } else if self.match_kind(&TokenKind::NotEq) {
                BinaryOp::NotEq
            } else {
                break;
            };
            let right = self.comparison()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.term()?;
        loop {
            let op = if self.match_kind(&TokenKind::Lt) {
                BinaryOp::Lt
            } else if self.match_kind(&TokenKind::LtEq) {
                BinaryOp::LtEq
            } else if self.match_kind(&TokenKind::Gt) {
                BinaryOp::Gt
            } else if self.match_kind(&TokenKind::GtEq) {
                BinaryOp::GtEq
            } else {
                break;
            };
            let right = self.term()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.factor()?;
        loop {
            let op = if self.match_kind(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_kind(&TokenKind::Minus) {
                BinaryOp::Sub
            } else {
                break;
            };
            let right = self.factor()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn factor(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.unary()?;
        loop {
            let op = if self.match_kind(&TokenKind::Star) {
                BinaryOp::Mul
            } else if self.match_kind(&TokenKind::Slash) {
                BinaryOp::Div
            } else {
                break;
            };
            let right = self.unary()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.match_kind(&TokenKind::Not) {
            let expr = self.unary()?;
            Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(expr) })
        } else if self.match_kind(&TokenKind::Minus) {
            let expr = self.unary()?;
            Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(expr) })
        } else {
            self.postfix()
        }
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.primary()?;
        loop {
            if self.match_kind(&TokenKind::LParen) {
                let mut args = Vec::new();
                if !self.check(&TokenKind::RParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.match_kind(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RParen, "after call arguments")?;
                expr = Expr::Call { callee: Box::new(expr), args };
            } else if self.match_kind(&TokenKind::LBracket) {
                let index = self.expression()?;
                self.expect(&TokenKind::RBracket, "to close index expression")?;
                expr = Expr::Index { object: Box::new(expr), index: Box::new(index) };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Int(v)))
            }
            TokenKind::Float(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(v)))
            }
            TokenKind::Str(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Str(v)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.expression()?;
                self.expect(&TokenKind::RParen, "after grouped expression")?;
                Ok(Expr::Grouping(Box::new(expr)))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                if !self.check(&TokenKind::RBracket) {
                    loop {
                        elements.push(self.expression()?);
                        if !self.match_kind(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RBracket, "to close array literal")?;
                Ok(Expr::Array(elements))
            }
            other => Err(self.error(&format!("unexpected {}", other))),
        }
    }
}
