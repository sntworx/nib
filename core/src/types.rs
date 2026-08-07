use std::fmt;

use crate::ast::ParseError;
use crate::lexer::LexError;
use crate::runtime::RuntimeError;

#[derive(Debug)]
pub enum Error {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeError),
    // Wraps any of the three above when it happened while lexing/parsing/
    // running source passed to `Nib::include` rather than the main script -
    // included sources are lexed/parsed independently and each start
    // counting from line 1 relative to their own string (see `Nib::run`),
    // so line:col alone can't tell a host which source an error came from
    // when there's more than one candidate. No further identification than
    // that (no per-include name/label) since `include` doesn't have or want
    // a namespacing concept.
    Included(Box<Error>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Lex(e) => write!(f, "{}", e),
            Error::Parse(e) => write!(f, "{}", e),
            Error::Runtime(e) => write!(f, "{}", e),
            Error::Included(e) => write!(f, "{} (in included code)", e),
        }
    }
}

impl From<LexError> for Error {
    fn from(e: LexError) -> Self {
        Error::Lex(e)
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}

impl From<RuntimeError> for Error {
    fn from(e: RuntimeError) -> Self {
        Error::Runtime(e)
    }
}
