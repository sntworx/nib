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

    // Queues `source` to be lexed, parsed, and run before the main script -
    // the mechanism a host uses to load a prepared or custom nib-authored
    // library. Deliberately infallible and deferred entirely to `run()`:
    // this just stores the string, so a host never needs a try/catch around
    // `include()` itself in a binding language - the only place errors can
    // ever surface, for included code or the main script alike, is `run()`.
    //
    // Included sources are lexed/parsed independently of each other and of
    // the main script (each starting fresh at line 1 relative to its own
    // string), so a lex/parse/runtime error inside one still reports
    // accurate line/col positions, never shifted by whatever came before it
    // - the reason this stores source strings and re-parses in `run()`
    // rather than concatenating raw text up front.
    //
    // Included sources run in `run()` in the order `include()` was called,
    // then the main script. Later definitions (a later include, or the main
    // script) silently shadow earlier ones in the shared global scope -
    // ordinary `Environment::define` overwrite behavior, same as redefining
    // any other global, not a namespaced/collision-checked import.
    //
    // Deliberately lexed ignoring `disable_keywords`: that restricts what an
    // untrusted *user* script (the thing passed to `parse()`) can do, but
    // source passed to `include()` is chosen by the host itself, same trust
    // level as a `register_func` closure. A host that disables `while` for
    // user scripts shouldn't have its own stdlib silently fail to parse
    // just because the stdlib happens to use `while` internally.
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
