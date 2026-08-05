use std::fmt;

use crate::ast::ParseError;
use crate::lexer::LexError;
use crate::runtime::RuntimeError;

#[derive(Debug)]
pub enum Error {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Lex(e) => write!(f, "{}", e),
            Error::Parse(e) => write!(f, "{}", e),
            Error::Runtime(e) => write!(f, "{}", e),
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
