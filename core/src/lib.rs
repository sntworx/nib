mod ast;
mod lexer;
mod runtime;
mod types;

use ast::Ast;
use lexer::{Lexer, TokenKind};
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

impl Default for Nib {
    fn default() -> Self {
        Self::new()
    }
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

    // All-or-nothing: every name is validated before any is recorded, so a
    // rejected call leaves the restriction set exactly as it was rather than
    // half-applied.
    pub fn disable_keywords(&mut self, keywords: Vec<&str>) -> Result<(), Error> {
        for keyword in &keywords {
            if TokenKind::from_keyword(keyword).is_none() {
                return Err(Error::UnknownKeyword(keyword.to_string()));
            }
        }
        for keyword in keywords {
            if !self.disabled_keywords.iter().any(|k| k == keyword) {
                self.disabled_keywords.push(keyword.to_string());
            }
        }
        Ok(())
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

    // Resolved before the includes run so that calling run() without a parsed
    // script is a clean no-op error, not one that has already executed and
    // cleared the queued includes on its way out.
    pub fn run(&mut self) -> Result<(), Error> {
        let Some(ast) = self.ast.as_ref() else {
            return Err(Error::NotParsed);
        };

        for source in &self.included {
            Self::run_source(&mut self.interpreter, source, self.config.max_parse_depth)
                .map_err(|e| Error::Included(Box::new(e)))?;
        }
        self.included.clear();

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
