//! `nib` is a small, embeddable scripting language with C-like syntax
//! (`var`, `if`/`else`, `while`, `for`, top-level `func`s with no closures,
//! arrays, maps) and a tree-walking interpreter.
//!
//! Nothing is pre-bound by default: a host opts a script into native
//! functions via [`Nib::register_func`], and can strip specific keywords out
//! of the language for a given script via [`Nib::disable_keywords`] (e.g.
//! dropping `while`/`for` to rule out unbounded loops). That makes it a fit
//! for running untrusted or user-authored logic inside a larger
//! application — plugin scripting, rules/workflow engines, user-defined
//! formulas — where a script should only ever touch what was explicitly
//! exposed to it.
//!
//! ```
//! use nib_lang::{Nib, Value};
//! use std::cell::RefCell;
//! use std::rc::Rc;
//!
//! let mut nib = Nib::new();
//!
//! // A native function the script can call.
//! nib.register_func("make_pair", |args: &[Value]| match args {
//!     [a, b] => Ok(Value::array(vec![a.clone(), b.clone()])),
//!     _ => Err("make_pair() expects two arguments".to_string()),
//! });
//!
//! // `nib`-authored library code, run before the main script.
//! nib.include("func double(x) { return x * 2; }");
//!
//! // Restrict the language surface for this script.
//! nib.disable_keywords(vec!["while"]).unwrap();
//!
//! // A native function to capture what the script reports, the same
//! // pattern a host's own `print` would use.
//! let out = Rc::new(RefCell::new(Vec::new()));
//! let out_clone = Rc::clone(&out);
//! nib.register_func("out", move |args: &[Value]| {
//!     out_clone.borrow_mut().push(args[0].clone());
//!     Ok(Value::Null)
//! });
//!
//! nib.parse("out(make_pair(double(21), 1));").unwrap();
//! nib.run().unwrap();
//!
//! assert_eq!(out.borrow()[0], Value::array(vec![Value::Int(42), Value::Int(1)]));
//! ```
//!
//! Sandbox limits (call depth, step count, string/array/map size, ...) are
//! configured via [`Config`], passed to [`Nib::with_config`]. Every fallible
//! entry point returns [`Error`].

mod ast;
mod lexer;
mod runtime;
mod types;

use ast::Ast;
use lexer::{Lexer, TokenKind};
use runtime::Interpreter;

pub use runtime::Value;
pub use types::{Config, Error};

/// An embeddable `nib` interpreter instance.
///
/// Owns a persistent interpreter, so native functions registered via
/// [`register_func`](Nib::register_func) and script-defined global state
/// (e.g. top-level `var`s) survive across repeated
/// [`parse`](Nib::parse)/[`run`](Nib::run) calls. A typical flow: register
/// any native functions, optionally [`include`](Nib::include) shared `nib`
/// library code, then [`parse`](Nib::parse) and [`run`](Nib::run) a script.
pub struct Nib {
    ast: Option<Ast>,
    included: Vec<String>,
    interpreter: Interpreter,
    disabled_keywords: Vec<String>,
    config: Config,
}

impl Default for Nib {
    /// Equivalent to [`Nib::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Nib {
    /// Creates a `Nib` instance with [`Config::default`] sandbox limits.
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Creates a `Nib` instance with custom sandbox limits.
    ///
    /// See [`Config`] for what each limit bounds and why the defaults are
    /// sized the way they are.
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::{Config, Nib};
    ///
    /// let mut config = Config::default();
    /// config.max_steps = 1_000;
    ///
    /// let nib = Nib::with_config(config);
    /// ```
    pub fn with_config(config: Config) -> Self {
        Nib {
            ast: None,
            included: vec![],
            interpreter: Interpreter::new(&config),
            disabled_keywords: vec![],
            config,
        }
    }

    /// Registers a native function under `name`, callable from a parsed
    /// script.
    ///
    /// `f` receives the evaluated argument [`Value`]s and returns either a
    /// result `Value` or a plain error message — there's no access to the
    /// interpreter's source position from inside `f`, so a failure is just a
    /// `String`; the interpreter attaches position info at the call site.
    /// Nothing is pre-bound by default, so this is the only way a script
    /// gains any capability beyond the language itself.
    ///
    /// A native function is bound in the same global scope as everything
    /// else, so it composes for free: it can be shadowed by the script or by
    /// an [`include`](Nib::include)d one, and is visible to every function
    /// the script defines.
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::{Nib, Value};
    ///
    /// let mut nib = Nib::new();
    /// nib.register_func("double", |args: &[Value]| match args {
    ///     [Value::Int(n)] => Ok(Value::Int(n * 2)),
    ///     _ => Err("double() expects one int argument".to_string()),
    /// });
    ///
    /// nib.parse("double(21);")?;
    /// nib.run()?;
    /// # Ok::<(), nib_lang::Error>(())
    /// ```
    pub fn register_func(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&[Value]) -> Result<Value, String> + 'static,
    ) {
        self.interpreter.register_native(name, f);
    }

    /// Restricts the language surface by disabling specific keywords for
    /// scripts subsequently parsed via [`parse`](Nib::parse) — e.g. dropping
    /// `while`/`for` to rule out unbounded loops.
    ///
    /// All-or-nothing: every name is validated as a real `nib` keyword
    /// before any of them is applied, so a rejected call changes nothing
    /// rather than silently leaving the language less restricted than the
    /// caller believes.
    ///
    /// Only affects the main script passed to `parse()` — source passed to
    /// [`include`](Nib::include) is chosen by the host itself, the same
    /// trust level as a `register_func` closure, and is lexed without
    /// restriction.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownKeyword`] if any name in `keywords` isn't a
    /// real `nib` keyword (e.g. a typo like `"Whlie"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::Nib;
    ///
    /// let mut nib = Nib::new();
    /// nib.disable_keywords(vec!["while"])?;
    ///
    /// assert!(nib.parse("while true { }").is_err());
    /// # Ok::<(), nib_lang::Error>(())
    /// ```
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

    /// Lexes and parses `source`, replacing any previously parsed script.
    ///
    /// Must succeed before [`run`](Nib::run) is called.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Lex`] on a lexing failure (e.g. an unterminated
    /// string or a malformed literal) or [`Error::Parse`] on a syntax error
    /// or on exceeding [`Config::max_parse_depth`]. Match `_` on either
    /// variant and read the message via `Display`, since the wrapped error
    /// types aren't nameable outside this crate.
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::Nib;
    ///
    /// let mut nib = Nib::new();
    /// nib.parse("var x = 1 + 2;")?;
    /// nib.run()?;
    /// # Ok::<(), nib_lang::Error>(())
    /// ```
    pub fn parse(&mut self, source: &str) -> Result<(), Error> {
        let mut lexer = Lexer::new(source, &self.disabled_keywords);
        let tokens = lexer.tokenize()?;
        self.ast = Some(Ast::parse(tokens, self.config.max_parse_depth)?);
        Ok(())
    }

    /// Queues `source` as shared `nib` library code to run before the main
    /// script — e.g. a host's own `nib`-authored helper functions.
    ///
    /// Infallible — it only stores the source string. Every included source
    /// is lexed, parsed, and run in call order the next time
    /// [`run`](Nib::run) is invoked, before the main script, against the
    /// same persistent interpreter [`register_func`](Nib::register_func)
    /// bindings use. Top-level `func`s (and other global state) defined
    /// there land in the same global scope the main script runs in, so it
    /// can call them directly — with no closures and no module system, this
    /// is `nib`'s only way to share code between scripts. Later definitions
    /// (a later `include`, or the main script itself) silently shadow
    /// earlier ones, ordinary global-scope overwrite rather than a
    /// namespaced import.
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::{Nib, Value};
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    ///
    /// let mut nib = Nib::new();
    /// nib.include("func double(x) { return x * 2; }");
    ///
    /// let result = Rc::new(Cell::new(0));
    /// let result_clone = Rc::clone(&result);
    /// nib.register_func("out", move |args: &[Value]| {
    ///     if let [Value::Int(n)] = args {
    ///         result_clone.set(*n);
    ///     }
    ///     Ok(Value::Null)
    /// });
    ///
    /// nib.parse("out(double(21));")?;
    /// nib.run()?;
    ///
    /// assert_eq!(result.get(), 42);
    /// # Ok::<(), nib_lang::Error>(())
    /// ```
    pub fn include(&mut self, source: impl Into<String>) {
        self.included.push(source.into());
    }

    /// Returns the most recently [`parse`](Nib::parse)d AST, if any.
    ///
    /// The returned type exposes no public API beyond `Debug` — this exists
    /// for debugging/tooling (e.g. printing a script's parsed structure),
    /// not for a host to inspect or manipulate parsed programs.
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::Nib;
    ///
    /// let mut nib = Nib::new();
    /// assert!(nib.ast().is_none());
    ///
    /// nib.parse("var x = 1;")?;
    /// assert!(nib.ast().is_some());
    /// # Ok::<(), nib_lang::Error>(())
    /// ```
    pub fn ast(&self) -> Option<&Ast> {
        self.ast.as_ref()
    }

    /// Runs every queued [`include`](Nib::include)d source, in call order,
    /// followed by the most recently [`parse`](Nib::parse)d script, against
    /// the persistent interpreter.
    ///
    /// Queued includes are only cleared after a fully successful run, so a
    /// mid-run failure doesn't silently drop them.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotParsed`] if called before a successful
    /// `parse()`. A failure while running the main script is
    /// [`Error::Runtime`]; a failure in an included source (a lex, parse,
    /// or runtime error - included sources are lexed and parsed here, not
    /// in `include()`) is wrapped in [`Error::Included`] so it isn't
    /// mistaken for a main-script error.
    ///
    /// # Examples
    ///
    /// ```
    /// use nib_lang::{Error, Nib};
    ///
    /// let mut nib = Nib::new();
    /// assert!(matches!(nib.run(), Err(Error::NotParsed)));
    ///
    /// nib.parse("var x = 1;")?;
    /// nib.run()?;
    /// # Ok::<(), nib_lang::Error>(())
    /// ```
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
