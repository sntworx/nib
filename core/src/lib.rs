mod ast;
mod lexer;
mod runtime;
mod types;

use ast::Ast;
use lexer::Lexer;
use runtime::Interpreter;
use types::Error;

pub use runtime::Value;
pub use types::Config;

pub struct Nib {
    ast: Option<Ast>,
    included: Vec<String>,
    interpreter: Interpreter,
    disabled_keywords: Vec<String>,
    config: Config,
}

impl Nib {
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    pub fn with_config(config: Config) -> Self {
        Nib {
            ast: None,
            included: vec![],
            interpreter: Interpreter::new(&config),
            disabled_keywords: vec![],
            config,
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
        self.ast = Some(Ast::parse(tokens, self.config.max_parse_depth)?);
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
            Self::run_source(&mut self.interpreter, source, self.config.max_parse_depth)
                .map_err(|e| Error::Included(Box::new(e)))?;
        }
        self.included.clear();

        let ast = self.ast.as_ref().expect("parse must be called before run");
        self.interpreter.run(ast)?;
        Ok(())
    }

    fn run_source(
        interpreter: &mut Interpreter,
        source: &str,
        max_parse_depth: usize,
    ) -> Result<(), Error> {
        let mut lexer = Lexer::new(source, &[]);
        let tokens = lexer.tokenize()?;
        let ast = Ast::parse(tokens, max_parse_depth)?;
        interpreter.run(&ast)?;
        Ok(())
    }
}
