mod ast;
mod lexer;
mod runtime;
mod types;

use ast::Ast;
use lexer::Lexer;
use runtime::Interpreter;
use types::Error;

pub use runtime::Value;

pub struct Nib {
    ast: Option<Ast>,
    included: Vec<String>,
    interpreter: Interpreter,
    disabled_keywords: Vec<String>,
}

impl Nib {
    pub fn new() -> Self {
        Nib {
            ast: None,
            included: vec![],
            interpreter: Interpreter::new(),
            disabled_keywords: vec![],
        }
    }

    pub fn register_func(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&[Value]) -> Result<Value, String> + 'static,
    ) {
        self.interpreter.register_native(name, f);
    }

    pub fn disable_keywords(&mut self, keywords: Vec<&str>) {
        for keyword in keywords {
            if !self.disabled_keywords.iter().any(|k| k == keyword) {
                self.disabled_keywords.push(keyword.to_string());
            }
        }
    }

    pub fn parse(&mut self, source: &str) -> Result<(), Error> {
        let mut lexer = Lexer::new(source, &self.disabled_keywords);
        let tokens = lexer.tokenize()?;
        self.ast = Some(Ast::parse(tokens)?);
        Ok(())
    }

    pub fn include(&mut self, source: impl Into<String>) {
        self.included.push(source.into());
    }

    pub fn ast(&self) -> Option<&Ast> {
        self.ast.as_ref()
    }

    pub fn run(&mut self) -> Result<(), Error> {
        for source in &self.included {
            Self::run_source(&mut self.interpreter, source)
                .map_err(|e| Error::Included(Box::new(e)))?;
        }
        self.included.clear();

        let ast = self.ast.as_ref().expect("parse must be called before run");
        self.interpreter.run(ast)?;
        Ok(())
    }

    fn run_source(interpreter: &mut Interpreter, source: &str) -> Result<(), Error> {
        let mut lexer = Lexer::new(source, &[]);
        let tokens = lexer.tokenize()?;
        let ast = Ast::parse(tokens)?;
        interpreter.run(&ast)?;
        Ok(())
    }
}
