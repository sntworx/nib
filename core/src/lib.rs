mod lexer;
mod ast;
mod runtime;
mod types;

use ast::Ast;
use lexer::Lexer;
use runtime::Interpreter;
use types::Error;

pub use runtime::Value;

pub struct Lame {
    ast: Option<Ast>,
    interpreter: Interpreter,
}

impl Lame {
    pub fn new() -> Self {
        Lame { ast: None, interpreter: Interpreter::new() }
    }

    // Binds a Rust function into the global scope under `name`, callable from
    // Lame scripts like any other function (e.g. `lame.register("print", ...)`
    // makes `print(...)` resolve instead of erroring as undefined). Can be
    // called any time before `run`.
    pub fn register(&mut self, name: impl Into<String>, f: impl Fn(&[Value]) -> Result<Value, String> + 'static) {
        self.interpreter.register_native(name, f);
    }

    pub fn parse(&mut self, source: &str) -> Result<(), Error> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize()?;
        self.ast = Some(Ast::parse(tokens)?);
        Ok(())
    }

    pub fn ast(&self) -> Option<&Ast> {
        self.ast.as_ref()
    }

    pub fn run(&mut self) -> Result<(), Error> {
        let ast = self.ast.as_ref().expect("parse must be called before run");
        self.interpreter.run(ast)?;
        Ok(())
    }
}
