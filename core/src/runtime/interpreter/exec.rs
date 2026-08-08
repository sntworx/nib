// Statement execution. Everything here returns `Flow` so `break`/`continue`/
// `return` can propagate up to whichever construct intercepts them.

use crate::ast::types::{
    AstNode, AstNodeKind, ForInStmt, ForStmt, FuncDecl, IfStmt, MatchStmt, TryStmt, VarAssign,
    WhileStmt,
};
use crate::runtime::helpers::{is_truthy, values_equal};
use crate::runtime::types::{Function, RuntimeError, Value};
use std::rc::Rc;

use super::{Flow, Interpreter};

impl Interpreter {
    fn exec(&mut self, node: &AstNode) -> Result<Flow, RuntimeError> {
        if self.should_exit {
            return Ok(Flow::Normal);
        }
        self.current_pos = (node.line, node.col);
        self.tick()?;
        match &node.kind {
            AstNodeKind::VarAssign(VarAssign { name, value }) => {
                let value = self.eval(value)?;
                self.env.define(name.clone(), value);
                Ok(Flow::Normal)
            }
            AstNodeKind::ExprStmt(expr) => {
                self.eval(expr)?;
                Ok(Flow::Normal)
            }
            AstNodeKind::If(if_stmt) => self.exec_if(if_stmt),
            AstNodeKind::Block(nodes) => self.exec_block(nodes),
            AstNodeKind::FuncDecl(FuncDecl { name, params, body }) => {
                let function = Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                };
                self.env
                    .define(name.clone(), Value::Function(Rc::new(function)));
                Ok(Flow::Normal)
            }
            AstNodeKind::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.eval(expr)?,
                    None => Value::Null,
                };
                Ok(Flow::Return(value))
            }
            AstNodeKind::While(while_stmt) => self.exec_while(while_stmt),
            AstNodeKind::For(for_stmt) => self.exec_for(for_stmt),
            AstNodeKind::ForIn(for_in_stmt) => self.exec_for_in(for_in_stmt),
            AstNodeKind::Match(match_stmt) => self.exec_match(match_stmt),
            AstNodeKind::Break => Ok(Flow::Break),
            AstNodeKind::Continue => Ok(Flow::Continue),
            AstNodeKind::Try(try_stmt) => self.exec_try(try_stmt),
            AstNodeKind::Throw(expr) => {
                let value = self.eval(expr)?;
                Err(self.throw_error(value))
            }
            AstNodeKind::Exit => {
                self.should_exit = true;
                Ok(Flow::Normal)
            }
        }
    }

    fn exec_if(&mut self, if_stmt: &IfStmt) -> Result<Flow, RuntimeError> {
        if is_truthy(&self.eval(&if_stmt.condition)?) {
            self.exec_block(&if_stmt.then_branch)
        } else {
            match &if_stmt.else_branch {
                Some(else_branch) => self.exec_block(else_branch),
                None => Ok(Flow::Normal),
            }
        }
    }

    // Arms tested top-to-bottom with `==`'s equality; first match wins, no fallthrough.
    fn exec_match(&mut self, match_stmt: &MatchStmt) -> Result<Flow, RuntimeError> {
        let subject = self.eval(&match_stmt.subject)?;
        for arm in &match_stmt.arms {
            let pattern = self.eval(&arm.pattern)?;
            if values_equal(&subject, &pattern) {
                return self.exec_block(&arm.body);
            }
        }
        match &match_stmt.default_branch {
            Some(default_branch) => self.exec_block(default_branch),
            None => Ok(Flow::Normal),
        }
    }

    // Only `try_block`'s own execution is guarded - an error raised inside
    // `catch_block` propagates normally rather than being caught by its own
    // try. Every RuntimeError is catchable except a fatal one (a blown
    // max_steps budget - see RuntimeError::fatal), which propagates straight
    // through. Call-depth and size-limit errors stay catchable and can't be
    // used to bypass the budget they guard: call_function decrements
    // call_depth before propagating, and the size checks hold no counter at
    // all, so both leave the interpreter able to run the catch block
    // normally.
    fn exec_try(&mut self, try_stmt: &TryStmt) -> Result<Flow, RuntimeError> {
        match self.exec_block(&try_stmt.try_block) {
            Ok(flow) => Ok(flow),
            Err(err) if err.fatal => Err(err),
            Err(err) => {
                let caught = err.value.unwrap_or(Value::Str(err.message));
                self.env.push_scope();
                self.env.define(try_stmt.catch_var.clone(), caught);
                let flow = self.exec_all(&try_stmt.catch_block);
                self.env.pop_scope();
                flow
            }
        }
    }

    fn exec_while(&mut self, while_stmt: &WhileStmt) -> Result<Flow, RuntimeError> {
        loop {
            if self.should_exit {
                return Ok(Flow::Normal);
            }
            self.tick()?;
            if !is_truthy(&self.eval(&while_stmt.condition)?) {
                return Ok(Flow::Normal);
            }
            match self.exec_block(&while_stmt.body)? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                flow @ Flow::Return(_) => return Ok(flow),
            }
        }
    }

    // `init` lives in its own scope for the loop's whole lifetime (not
    // per-iteration, unlike `body`), so it's visible to condition/post/body
    // but doesn't leak into the surrounding scope.
    fn exec_for(&mut self, for_stmt: &ForStmt) -> Result<Flow, RuntimeError> {
        self.env.push_scope();
        let result = self.exec_for_body(for_stmt);
        self.env.pop_scope();
        result
    }

    fn exec_for_body(&mut self, for_stmt: &ForStmt) -> Result<Flow, RuntimeError> {
        if let Some(init) = &for_stmt.init {
            self.exec(init)?;
        }
        loop {
            if self.should_exit {
                return Ok(Flow::Normal);
            }
            self.tick()?;
            let should_continue = match &for_stmt.condition {
                Some(condition) => is_truthy(&self.eval(condition)?),
                None => true,
            };
            if !should_continue {
                return Ok(Flow::Normal);
            }

            match self.exec_block(&for_stmt.body)? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                flow @ Flow::Return(_) => return Ok(flow),
            }

            if let Some(post) = &for_stmt.post {
                self.eval(post)?;
            }
        }
    }

    // `iterable` is evaluated once up front, so reassigning it mid-loop
    // doesn't change what's iterated; each element is handed to the body by
    // value, so mutating the loop variable never writes back into the array.
    fn exec_for_in(&mut self, for_in_stmt: &ForInStmt) -> Result<Flow, RuntimeError> {
        let iterable = self.eval(&for_in_stmt.iterable)?;
        let items = match iterable {
            Value::Array(items) => items,
            other => {
                return Err(self.error(format!("cannot iterate over {}", other.type_name())));
            }
        };

        for item in items.iter() {
            if self.should_exit {
                return Ok(Flow::Normal);
            }
            self.tick()?;
            self.env.push_scope();
            self.env.define(for_in_stmt.var_name.clone(), item.clone());
            let flow = self.exec_all(&for_in_stmt.body);
            self.env.pop_scope();
            match flow? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                flow @ Flow::Return(_) => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_block(&mut self, nodes: &[AstNode]) -> Result<Flow, RuntimeError> {
        self.env.push_scope();
        let result = self.exec_all(nodes);
        self.env.pop_scope();
        result
    }

    // Runs statements in order, stopping early (without running the rest) as
    // soon as one of them returns/breaks/continues.
    pub(super) fn exec_all(&mut self, nodes: &[AstNode]) -> Result<Flow, RuntimeError> {
        for node in nodes {
            match self.exec(node)? {
                Flow::Normal => {}
                flow => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }
}
