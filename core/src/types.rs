use std::fmt;

use crate::ast::ParseError;
use crate::lexer::LexError;
use crate::runtime::RuntimeError;

pub struct Config {
    // Caps recursive function call depth; exceeding it is a RuntimeError
    // ("stack overflow: exceeded maximum call depth of {}"), not a native
    // stack overflow that would abort the process. 200 leaves ~4x margin
    // under the ~800 frames a release build actually survives on a
    // constrained 1MiB stack (wasm32's default, and small worker threads) -
    // the previous default of 1000 overflowed it outright, aborting the
    // host instead of erroring. Don't raise the default without
    // re-verifying against a small-stack thread.
    pub max_call_depth: usize,
    // Caps recursive-descent parser nesting so malformed/malicious input
    // can't overflow the real stack while parsing. Much lower than
    // `max_call_depth` since one grammar level burns several real stack
    // frames here, not one - 128 is verified safe with margin on a
    // constrained 1MiB stack (a small worker-thread stack, not just the
    // CLI's ~8MiB main thread); 1000 was not. Verified against release
    // builds - a debug build's much larger frames can overflow 1MiB while
    // still under this limit. Don't raise the default without
    // re-verifying against a small-stack thread.
    pub max_parse_depth: usize,
    // Caps total interpreter work per `run()` call (one tick per statement
    // executed and per loop iteration) so a script that loops forever (e.g.
    // `while true { }`) fails with a RuntimeError instead of hanging the
    // host process. Resets to 0 at the start of every `run()` call, so it's
    // a per-run budget, not a lifetime total on a reused `Nib`/`Interpreter`.
    // 1,000,000 fails a trivial infinite loop in a fraction of a second
    // while leaving generous headroom for legitimate loops over thousands
    // of elements.
    pub max_steps: usize,
    // Caps a single string's length (character count, matching `Str::len()`'s
    // char-count-not-byte-count convention), checked at every point a string
    // is constructed or grown (literals, concatenation, methods that return
    // a string). Guards against unbounded memory growth via e.g. `s += "x";`
    // in a loop.
    pub max_string_length: usize,
    // Caps a single array's element count, checked at every point an array
    // is constructed or grown (literals, `push()`, methods that return an
    // array like `chars()`/`keys()`/`values()`). Guards against unbounded
    // memory growth via e.g. `arr.push(x);` in a loop.
    pub max_array_length: usize,
    // Caps a single map's entry count, checked whenever a *new* key is
    // inserted (literals, and `m[newKey] = x`) - overwriting an existing key
    // never grows the map, so it's never rejected regardless of this limit.
    pub max_map_size: usize,
    // Caps how deeply arrays/maps may nest inside each other (`[[[1]]]` is 3).
    // Unlike the other limits this one guards the *native stack*, not memory:
    // every recursive walk over a value - Display, equality, the host
    // bindings' conversion, Drop - costs one real stack frame per level, so a
    // deep enough value aborts the process. Drop is why this has to be a
    // build-time limit rather than a check inside each walk: a destructor
    // can't return an error or be caught. 128 matches `max_parse_depth` and
    // the bindings' own conversion cap, so nothing here nests deeper than 128.
    pub max_value_depth: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_call_depth: 200,
            max_parse_depth: 128,
            max_steps: 1_000_000,
            max_string_length: 1_000_000,
            max_array_length: 1_000_000,
            max_map_size: 1_000_000,
            max_value_depth: 128,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeError),
    // An error from `Nib::include`d source rather than the main script -
    // each include starts at line 1, so line:col alone can't tell them apart.
    Included(Box<Error>),
    // `run()` with no parsed script. An API misuse rather than a fault in any
    // source, so it carries no line/col - but still an Err, since a panic here
    // would abort the host across the PHP/wasm FFI boundary instead of
    // surfacing as a catchable exception.
    NotParsed,
    // `disable_keywords` given something that isn't a keyword. Rejected rather
    // than ignored because it's a security control: silently accepting a typo
    // ("Whlie") leaves the host believing it restricted the language when it
    // didn't. Host-supplied, so echoing the name back is safe.
    UnknownKeyword(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Lex(e) => write!(f, "{}", e),
            Error::Parse(e) => write!(f, "{}", e),
            Error::Runtime(e) => write!(f, "{}", e),
            Error::Included(e) => write!(f, "{} (in included code)", e),
            Error::NotParsed => write!(f, "no script to run: call parse() before run()"),
            Error::UnknownKeyword(name) => {
                write!(f, "cannot disable '{}': not a nib keyword", name)
            }
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
