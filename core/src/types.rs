use std::fmt;

use crate::ast::ParseError;
use crate::lexer::LexError;
use crate::runtime::RuntimeError;

/// Sandbox limits for a [`Nib`](crate::Nib) instance, passed to
/// [`Nib::with_config`](crate::Nib::with_config).
///
/// Every field is sized for an embedded sandbox rather than a
/// maximum-plausible script — a host that needs more can raise any one
/// field in one line, while a host that didn't know it needed a limit still
/// gets one that holds. See [`Config::default`] for the concrete numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Caps recursive `nib` function-call depth; exceeding it is a
    /// `RuntimeError` ("stack overflow: exceeded maximum call depth of
    /// {}"), not a native stack overflow that would abort the process. The
    /// default leaves ~4x margin under the ~800 frames a release build
    /// actually survives on a constrained 1MiB stack (wasm32's default, and
    /// small worker threads) - don't raise the default without
    /// re-verifying against a small-stack thread.
    pub max_call_depth: usize,
    /// Caps recursive-descent parser nesting so malformed/malicious input
    /// can't overflow the real stack while parsing. Much lower than
    /// `max_call_depth` since one grammar level burns several real stack
    /// frames here, not one - verified safe with margin on a constrained
    /// 1MiB stack (a small worker-thread stack, not just a roomy ~8MiB main
    /// thread), and verified against *release* builds specifically -
    /// a debug build's much larger frames can overflow 1MiB while still
    /// under this limit. Don't raise the default without re-verifying
    /// against a small-stack thread.
    pub max_parse_depth: usize,
    /// Caps total interpreter work per `run()` call (one tick per statement
    /// executed and per loop iteration) so a script that loops forever
    /// (e.g. `while true { }`) fails with a `RuntimeError` instead of
    /// hanging the host process. Resets to 0 at the start of every `run()`
    /// call, so it's a per-run budget, not a lifetime total on a reused
    /// `Nib`/`Interpreter`. Unlike the other limits, exhausting it is not
    /// catchable by the script's own `try`/`catch`.
    pub max_steps: usize,
    /// Caps a single string's length (character count, matching
    /// `Str::len()`'s char-count-not-byte-count convention), checked at
    /// every point a string is constructed or grown (literals,
    /// concatenation, methods that return a string). Guards against
    /// unbounded memory growth via e.g. `s += "x";` in a loop.
    pub max_string_length: usize,
    /// Caps a single array's element count, checked at every point an
    /// array is constructed or grown (literals, `push()`, methods that
    /// return an array like `chars()`/`keys()`/`values()`). Guards against
    /// unbounded memory growth via e.g. `arr.push(x);` in a loop.
    pub max_array_length: usize,
    /// Caps a single map's entry count, checked whenever a *new* key is
    /// inserted (literals, and `m[newKey] = x`) - overwriting an existing
    /// key never grows the map, so it's never rejected regardless of this
    /// limit.
    pub max_map_size: usize,
    /// Caps how deeply arrays/maps may nest inside each other (`[[[1]]]`
    /// is 3). Unlike the other limits this one guards the *native stack*,
    /// not memory: every recursive walk over a value - `Display`,
    /// equality, a host converting it to its own representation, `Drop` -
    /// costs one real stack frame per level, so a deep enough value aborts
    /// the process. Checked when a value is built, not on each walk.
    pub max_value_depth: usize,
    /// Caps the *total* values in one array/map tree, counting nested ones
    /// - `[[1, 2], [3]]` is 6. The per-container limits above only measure
    /// one level, which copy-on-write makes insufficient on its own:
    /// `a = [a, a]` shares one physical copy of `a`, so it costs almost no
    /// memory and stays 2 elements long, yet doubles what printing,
    /// comparing, or converting the value to a host language has to walk.
    /// Defaults to 10x the per-container limits, so ordinary flat data
    /// still hits those (and their clearer messages) first.
    pub max_value_nodes: usize,
}

impl Default for Config {
    /// Defaults sized for an embedded sandbox: `max_call_depth` 200,
    /// `max_parse_depth` 128, `max_steps` 100,000, `max_string_length`
    /// 65,536, `max_array_length` 10,000, `max_map_size` 10,000,
    /// `max_value_depth` 64, `max_value_nodes` 100,000.
    fn default() -> Self {
        Config {
            max_call_depth: 200,
            max_parse_depth: 128,
            max_steps: 100_000,
            max_string_length: 65_536,
            max_array_length: 10_000,
            max_map_size: 10_000,
            max_value_depth: 64,
            max_value_nodes: 100_000,
        }
    }
}

/// The error type returned by [`Nib::parse`](crate::Nib::parse) and
/// [`Nib::run`](crate::Nib::run).
///
/// `Lex`/`Parse`/`Runtime` wrap types that aren't nameable from outside
/// this crate, deliberately - match them with `_` and read the message via
/// `Display` instead of destructuring them.
#[derive(Debug)]
pub enum Error {
    /// A lexing error in the source passed to `parse()`.
    Lex(LexError),
    /// A parse error in the source passed to `parse()`.
    Parse(ParseError),
    /// A runtime error raised while executing a script.
    Runtime(RuntimeError),
    /// An error from `Nib::include`d source rather than the main script,
    /// wrapping whichever of the other variants actually occurred - each
    /// included source starts at line 1, so line:col alone can't tell an
    /// included error apart from a main-script one.
    Included(Box<Error>),
    /// [`run()`](crate::Nib::run) was called without a preceding successful
    /// [`parse()`](crate::Nib::parse). API misuse rather than a fault in any
    /// source, so it carries no line/col - but still an `Err`, since a panic
    /// here could abort or poison a host embedding this crate across an FFI
    /// boundary instead of surfacing as a catchable exception.
    NotParsed,
    /// [`disable_keywords`](crate::Nib::disable_keywords) was given a name
    /// that isn't a real `nib` keyword. Rejected rather than ignored because
    /// it's a security control: silently accepting a typo ("Whlie") leaves
    /// the host believing it restricted the language when it didn't.
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
