//! Shared harness for the integration suite.
//!
//! Everything here goes through `nib_core`'s public API only - scripts report
//! results by calling `out(...)`, which the harness captures. Internals that
//! can't be reached this way are unit-tested next to the module instead (see
//! `src/**/ *_tests.rs`).

#![allow(dead_code)] // each tests/<dir>/main.rs uses a different subset

use nib_core::{Config, Nib, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Builds a `Nib` with `out(...)` bound, returning it alongside the sink the
/// script writes into. Use directly when a test needs to register extra
/// natives or includes before parsing.
pub fn harness(config: Config) -> (Nib, Rc<RefCell<Vec<String>>>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&log);
    let mut nib = Nib::with_config(config);
    nib.register_func("out", move |args: &[Value]| {
        let line = args
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        sink.borrow_mut().push(line);
        Ok(Value::Null)
    });
    (nib, log)
}

/// Runs `src`, returning whatever it passed to `out(...)`.
pub fn run(src: &str) -> Result<Vec<String>, String> {
    run_with(Config::default(), src)
}

pub fn run_with(config: Config, src: &str) -> Result<Vec<String>, String> {
    let (mut nib, log) = harness(config);
    nib.parse(src).map_err(|e| e.to_string())?;
    nib.run().map_err(|e| e.to_string())?;
    let out = log.borrow().clone();
    Ok(out)
}

/// Runs `src` expecting failure, returning the error message.
#[track_caller]
pub fn err(src: &str) -> String {
    err_with(Config::default(), src)
}

#[track_caller]
pub fn err_with(config: Config, src: &str) -> String {
    match run_with(config, src) {
        Err(e) => e,
        Ok(out) => panic!(
            "expected an error, but the script succeeded with: {:?}",
            out
        ),
    }
}

/// Runs `src` on a thread with an explicit stack size.
///
/// Recursion tests need this: a debug build's stack frames are roughly an
/// order of magnitude fatter than release, so `func f() { return f(); }`
/// overflows a default test thread and *aborts the process* before
/// `max_call_depth` can report it. Prefer lowering `max_call_depth` via
/// `Config` where possible; reserve this for pinning the real default.
pub fn run_stacked(stack_bytes: usize, src: &str) -> Result<Vec<String>, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(stack_bytes)
        .spawn(move || run(&src))
        .expect("spawn test thread")
        .join()
        .expect("test thread panicked")
}

/// `Config::default()` with one field overridden, for limit tests.
pub fn cfg(f: impl FnOnce(&mut Config)) -> Config {
    let mut c = Config::default();
    f(&mut c);
    c
}
