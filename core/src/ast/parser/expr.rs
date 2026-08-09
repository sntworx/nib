// The expression grammar, in descent order: `expression` -> `assignment` ->
// `ternary` -> `or` -> `and` -> `equality` -> `comparison` -> `term` ->
// `factor` -> `unary` -> `postfix` -> `primary`. Kept in one file on purpose -
// the chain is the precedence table, and splitting it mid-descent would scatter
// the grammar across files. Nothing here calls back into statement parsing.

use crate::ast::types::{BinaryOp, Expr, Literal, ParseError, UnaryOp};
use crate::lexer::TokenKind;

use super::Parser;

impl Parser {
    pub(super) fn expression(&mut self) -> Result<Expr, ParseError> {
        self.enter_nesting()?;
        let result = self.assignment();
        self.depth -= 1;
        result
    }

    fn assignment(&mut self) -> Result<Expr, ParseError> {
        // `++x`/`--x` desugar like their postfix counterparts below. Checked
        // before `self.or()` so the target is parsed at postfix precedence,
        // not swallowing a trailing expression like `++x + 1`.
        if self.match_kind(&TokenKind::PlusPlus) {
            let target = self.postfix()?;
            return self.incr_decr(target, BinaryOp::Add);
        }
        if self.match_kind(&TokenKind::MinusMinus) {
            let target = self.postfix()?;
            return self.incr_decr(target, BinaryOp::Sub);
        }

        let expr = self.ternary()?;

        // `x++`/`x--` desugar to `x = x + 1`/`x = x - 1` - not embeddable
        // mid-expression like `1 + x++`, same as compound assignment below.
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
        } else if self.match_kind(&TokenKind::PercentEq) {
            Some(BinaryOp::Mod)
        } else if self.match_kind(&TokenKind::Assign) {
            None
        } else {
            return Ok(expr);
        };

        // goes through `expression()` (not a direct `self.assignment()` call)
        // purely so chained assignment gets covered by its depth guard too
        let value = self.expression()?; // right-associative
        match expr {
            Expr::Ident(name) => {
                let value = match compound_op {
                    Some(op) => Expr::Binary {
                        op,
                        left: Box::new(Expr::Ident(name.clone())),
                        right: Box::new(value),
                    },
                    None => value,
                };
                Ok(Expr::Assign {
                    name,
                    value: Box::new(value),
                })
            }
            // unlike the Ident case above, the compound op isn't desugared
            // here into a Binary - the interpreter applies it directly so
            // `object`/`index` only get evaluated once (they may have side
            // effects, e.g. `arr[i()] += 1`)
            Expr::Index { object, index } => Ok(Expr::IndexAssign {
                object,
                index,
                op: compound_op,
                value: Box::new(value),
            }),
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

    // Both branches go through `expression()` rather than recursing straight
    // back into `ternary()`: that makes chains right-associative
    // (`a ? b : c ? d : e` groups to the right) and, more importantly, puts
    // nested ternaries under the same `max_parse_depth` guard everything else
    // has - they nest arbitrarily deep in one statement otherwise.
    fn ternary(&mut self) -> Result<Expr, ParseError> {
        let cond = self.or()?;
        if !self.match_kind(&TokenKind::Question) {
            return Ok(cond);
        }
        let then_expr = self.expression()?;
        self.expect(&TokenKind::Colon, "after '?' branch of ternary")?;
        let else_expr = self.expression()?;
        Ok(Expr::Ternary {
            cond: Box::new(cond),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        })
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.and()?;
        while self.match_kind(&TokenKind::OrOr) {
            let right = self.and()?;
            expr = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.equality()?;
        while self.match_kind(&TokenKind::AndAnd) {
            let right = self.equality()?;
            expr = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
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
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
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
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
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
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
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
            } else if self.match_kind(&TokenKind::Percent) {
                BinaryOp::Mod
            } else {
                break;
            };
            let right = self.unary()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.enter_nesting()?;
        let result = if self.match_kind(&TokenKind::Not) {
            self.unary().map(|expr| Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            })
        } else if self.match_kind(&TokenKind::Minus) {
            self.unary().map(|expr| Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            })
        } else {
            self.postfix()
        };
        self.depth -= 1;
        result
    }

    // Parses `(arg, arg, ...)` up to and including the closing `)` - shared
    // by call and method-call parsing, which both need the same possibly-
    // empty, comma-separated argument list.
    fn call_args(&mut self, context: &str) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                args.push(self.expression()?);
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, context)?;
        Ok(args)
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.primary()?;
        loop {
            if self.match_kind(&TokenKind::LParen) {
                let args = self.call_args("after call arguments")?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else if self.match_kind(&TokenKind::LBracket) {
                let index = self.expression()?;
                self.expect(&TokenKind::RBracket, "to close index expression")?;
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.match_kind(&TokenKind::Dot) {
                let method = self.expect_ident("after '.'")?;
                self.expect(&TokenKind::LParen, "after method name")?;
                let args = self.call_args("after method arguments")?;
                expr = Expr::MethodCall {
                    target: Box::new(expr),
                    method,
                    args,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    // Map literal keys are static: a string literal, or a bare identifier as
    // sugar for its own name (`{name: "Bob"}` == `{"name": "Bob"}`).
    fn map_key(&mut self) -> Result<String, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Str(s) => {
                self.advance();
                Ok(s)
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error("expected string literal or identifier as map key")),
        }
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
            TokenKind::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                if !self.check(&TokenKind::RBrace) {
                    loop {
                        let key = self.map_key()?;
                        self.expect(&TokenKind::Colon, "after map key")?;
                        let value = self.expression()?;
                        pairs.push((key, value));
                        if !self.match_kind(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RBrace, "to close map literal")?;
                Ok(Expr::Map(pairs))
            }
            other => Err(self.error(&format!("unexpected {}", other))),
        }
    }
}
