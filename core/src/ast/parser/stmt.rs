// Statement parsing. The only thing it needs from the expression grammar is
// `expression()` itself - every statement that takes a subject (`if`, `while`,
// `match`, `throw`, `return`, ...) parses it bare, with no parens of its own.

use crate::ast::types::{
    AstNode, AstNodeKind, Expr, ForInStmt, ForStmt, FuncDecl, IfStmt, MatchArm, MatchStmt,
    LetAssign, ParseError, TryStmt, WhileStmt,
};
use crate::lexer::TokenKind;

use super::Parser;

impl Parser {
    pub(super) fn statement(&mut self) -> Result<AstNode, ParseError> {
        let line = self.peek().line;
        let col = self.peek().col;

        let kind = if self.match_kind(&TokenKind::Let) {
            AstNodeKind::LetAssign(self.let_assign_stmt()?)
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
            self.for_stmt()?
        } else if self.check(&TokenKind::Match) {
            AstNodeKind::Match(self.match_stmt()?)
        } else if self.check(&TokenKind::Try) {
            AstNodeKind::Try(self.try_stmt()?)
        } else if self.match_kind(&TokenKind::Throw) {
            AstNodeKind::Throw(self.throw_stmt()?)
        } else if self.check(&TokenKind::Break) {
            self.break_stmt()?;
            AstNodeKind::Break
        } else if self.check(&TokenKind::Continue) {
            self.continue_stmt()?;
            AstNodeKind::Continue
        } else if self.match_kind(&TokenKind::Exit) {
            self.expect(&TokenKind::Semicolon, "after 'exit'")?;
            AstNodeKind::Exit
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

    fn let_assign_stmt(&mut self) -> Result<LetAssign, ParseError> {
        let name = self.expect_ident("after 'let'")?;
        self.expect(&TokenKind::Assign, "after variable name")?;
        let value = self.expression()?;
        self.expect(&TokenKind::Semicolon, "after variable declaration")?;
        Ok(LetAssign { name, value })
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
                self.enter_nesting()?;
                let nested = self.if_stmt();
                self.depth -= 1;
                let nested = nested?;
                Some(vec![AstNode {
                    kind: AstNodeKind::If(nested),
                    line,
                    col,
                }])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(IfStmt {
            condition,
            then_branch,
            else_branch,
        })
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

        Ok(WhileStmt {
            condition,
            body: body?,
        })
    }

    // C-style `for` requires parens, `for x in arr` never has them, so a
    // single-token lookahead after `for` tells the forms apart, no backtracking.
    fn for_stmt(&mut self) -> Result<AstNodeKind, ParseError> {
        self.expect(&TokenKind::For, "")?;
        if self.check(&TokenKind::LParen) {
            Ok(AstNodeKind::For(self.for_clauses_stmt()?))
        } else {
            Ok(AstNodeKind::ForIn(self.for_in_stmt()?))
        }
    }

    fn for_clauses_stmt(&mut self) -> Result<ForStmt, ParseError> {
        self.expect(&TokenKind::LParen, "after 'for'")?;

        let init = if self.match_kind(&TokenKind::Semicolon) {
            None
        } else {
            let line = self.peek().line;
            let col = self.peek().col;
            let kind = if self.match_kind(&TokenKind::Let) {
                AstNodeKind::LetAssign(self.let_assign_stmt()?)
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

        Ok(ForStmt {
            init,
            condition,
            post,
            body: body?,
        })
    }

    fn for_in_stmt(&mut self) -> Result<ForInStmt, ParseError> {
        let var_name = self.expect_ident("after 'for'")?;
        self.expect(&TokenKind::In, "after loop variable")?;
        // iterable is a bare expression, same style as if/while/match
        let iterable = self.expression()?;

        let saved_in_loop = self.in_loop;
        self.in_loop = true;
        let body = self.block();
        self.in_loop = saved_in_loop;

        Ok(ForInStmt {
            var_name,
            iterable,
            body: body?,
        })
    }

    fn match_stmt(&mut self) -> Result<MatchStmt, ParseError> {
        self.expect(&TokenKind::Match, "")?;
        // subject is a bare expression, same style as if/while
        let subject = self.expression()?;
        self.expect(&TokenKind::LBrace, "to start match body")?;

        let mut arms = Vec::new();
        let mut default_branch = None;
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            if self.match_kind(&TokenKind::Default) {
                if default_branch.is_some() {
                    return Err(self.error("match can only have one 'default' arm"));
                }
                default_branch = Some(self.block()?);
            } else {
                if default_branch.is_some() {
                    return Err(self.error("'default' must be the last arm in match"));
                }
                self.expect(&TokenKind::Case, "before match arm pattern")?;
                let pattern = self.expression()?;
                let body = self.block()?;
                arms.push(MatchArm { pattern, body });
            }
        }
        self.expect(&TokenKind::RBrace, "to close match body")?;

        Ok(MatchStmt {
            subject,
            arms,
            default_branch,
        })
    }

    // No bare `try` without `catch` - always paired, same as `for`'s
    // mandatory parens: one shape, no optional variant to special-case.
    fn try_stmt(&mut self) -> Result<TryStmt, ParseError> {
        self.expect(&TokenKind::Try, "")?;
        let try_block = self.block()?;
        self.expect(&TokenKind::Catch, "after 'try' block")?;
        // no parens around the caught variable, same bare style `for x in arr`
        // uses for its own single binding
        let catch_var = self.expect_ident("after 'catch'")?;
        let catch_block = self.block()?;
        Ok(TryStmt {
            try_block,
            catch_var,
            catch_block,
        })
    }

    fn throw_stmt(&mut self) -> Result<Expr, ParseError> {
        let value = self.expression()?;
        self.expect(&TokenKind::Semicolon, "after 'throw' value")?;
        Ok(value)
    }

    fn block(&mut self) -> Result<Vec<AstNode>, ParseError> {
        self.expect(&TokenKind::LBrace, "to start block")?;
        self.enter_nesting()?;

        let saved_at_top_level = self.at_top_level;
        self.at_top_level = false;
        let nodes = self.block_statements();
        self.at_top_level = saved_at_top_level;
        self.depth -= 1;
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
}
