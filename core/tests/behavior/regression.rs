//! Bugs that actually occurred. Each test names the failure it prevents, so a
//! future change that reintroduces one fails with an explanation rather than a
//! bare assertion.

use crate::common::{cfg, err, err_with, harness, run, run_with};
use nib_lang::{Config, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// `push` mutates in place after `detach_binding`, so there is no pre-mutation
/// value to roll back to. Checking the size limit *after* the fact left the
/// binding holding the `Null` that `Environment::take` put there, silently
/// destroying the array. The limit must be pre-checked.
#[test]
fn failed_push_at_the_size_limit_leaves_the_array_intact() {
    let c = cfg(|c| c.max_array_length = 3);
    let out = run_with(
        c,
        r#"var a = [1, 2, 3];
           try { a.push(4); } catch e { out("rejected"); }
           out(a, a.len());"#,
    )
    .unwrap();
    assert_eq!(out, ["rejected", "[1, 2, 3] 3"]);
}

/// Same hazard on the other in-place path: `a[i] = x` can deepen or grow the
/// tree (`a[0] = a`), and also detaches before mutating.
#[test]
fn failed_index_assignment_leaves_the_array_intact() {
    let c = cfg(|c| c.max_value_depth = 3);
    let out = run_with(
        c,
        r#"var a = [[[1]]];
           try { a[0] = a; } catch e { out("rejected"); }
           out(a);"#,
    )
    .unwrap();
    assert_eq!(out, ["rejected", "[[[1]]]"]);
}

/// Copy-on-write made the per-container limits insufficient: `a = [a, a]`
/// shares one physical copy, so it stays 2 elements long and costs almost no
/// memory while doubling the logical tree that Display/PartialEq/host
/// conversion each walk. 26 rounds printed 470 MB in 10s before max_value_nodes.
#[test]
fn shared_subtrees_cannot_explode_the_logical_tree() {
    let c = cfg(|c| c.max_value_nodes = 1000);
    let e = err_with(
        c,
        "var a = [1]; var i = 0; while i < 26 { a = [a, a]; i++; }",
    );
    assert!(e.contains("maximum total size"), "{}", e);
}

/// Negating i64::MIN is the one unary overflow, and it is reachable from
/// script (`math_abs` in stdlib/math.nib negates its argument).
#[test]
fn negating_i64_min_errors_instead_of_panicking() {
    let e = err("var m = -9223372036854775807 - 1; out(-m);");
    assert!(e.contains("integer overflow"), "{}", e);
}

#[test]
fn integer_overflow_is_checked_on_every_arithmetic_op() {
    for src in [
        "out(9223372036854775807 + 1);",
        "out(-9223372036854775807 - 2);",
        "out(9223372036854775807 * 2);",
    ] {
        assert!(err(src).contains("integer overflow"), "{}", src);
    }
}

/// Rust's `f64::from_str` accepts "inf"/"-inf"/"nan", which would smuggle a
/// non-finite value past the invariant every other float path enforces.
#[test]
fn to_float_rejects_non_finite_strings() {
    for s in ["inf", "-inf", "nan", "NaN", "infinity"] {
        let src = format!(r#"out("{}".to_float());"#, s);
        let e = err(&src);
        assert!(
            e.contains("floating-point overflow") || e.contains("cannot convert"),
            "{:?} gave an unexpected error: {}",
            s,
            e
        );
    }
    // ...while ordinary floats still work
    assert_eq!(run(r#"out("3.5".to_float());"#).unwrap(), ["3.5"]);
}

/// Duplicate keys in a map literal collapse to one entry: last value wins,
/// first position is kept.
#[test]
fn duplicate_map_literal_keys_collapse() {
    let out = run(r#"var m = {a: 1, b: 2, a: 3}; out(m, m.len());"#).unwrap();
    assert_eq!(out, ["{a: 3, b: 2} 2"]);
}

/// `run()` used to execute (and clear) queued includes on its way to failing,
/// so a run-before-parse mistake had already run the host's library code.
#[test]
fn run_before_parse_does_not_execute_includes() {
    let hits = Rc::new(RefCell::new(0));
    let counter = Rc::clone(&hits);
    let (mut nib, _log) = harness(Config::default());
    nib.register_func("touch", move |_: &[Value]| {
        *counter.borrow_mut() += 1;
        Ok(Value::Null)
    });
    nib.include("touch();");

    assert_eq!(
        nib.run().unwrap_err().to_string(),
        "no script to run: call parse() before run()"
    );
    assert_eq!(*hits.borrow(), 0, "include ran despite the missing script");

    // and the include is still queued for a later, valid run
    nib.parse("out(1);").unwrap();
    nib.run().unwrap();
    assert_eq!(*hits.borrow(), 1);
}

/// A successful `run()` clears the queue, so parsing and running a second
/// script must not re-run the includes and re-clobber their globals.
#[test]
fn includes_do_not_run_twice() {
    let (mut nib, log) = harness(Config::default());
    nib.include("var counter = 0;");
    nib.parse("counter = counter + 1; out(counter);").unwrap();
    nib.run().unwrap();
    nib.parse("counter = counter + 1; out(counter);").unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["1", "2"]);
}

/// The step-limit error is the one uncatchable RuntimeError: step_count is
/// already at the cap when it fires, so a catch block's first tick() would
/// re-error. Catching it only relocated the failure, never granted budget.
#[test]
fn step_limit_cannot_be_caught() {
    let c = cfg(|c| c.max_steps = 100);
    let e = err_with(c, r#"try { while true { } } catch e { out("caught"); }"#);
    assert!(e.contains("execution steps"), "{}", e);
}

/// ...while the limits that hold no counter open stay catchable.
#[test]
fn size_limits_remain_catchable() {
    let c = cfg(|c| c.max_array_length = 2);
    let out = run_with(
        c,
        r#"var a = [1, 2];
           try { a.push(3); } catch e { out("caught"); }
           out("still running");"#,
    )
    .unwrap();
    assert_eq!(out, ["caught", "still running"]);
}

/// `exit` is checked at the top of each loop construct's own iteration, not
/// only in exec(). Without that, `while true { exit; }` re-entered the loop
/// forever after the flag was set and burned the whole step budget, surfacing
/// as a step-limit error instead of a clean stop.
#[test]
fn exit_breaks_out_of_an_infinite_loop_cleanly() {
    let c = cfg(|c| c.max_steps = 1000);
    let out = run_with(
        c,
        r#"var i = 0; while true { i++; if i > 2 { exit; } } out("unreachable");"#,
    )
    .unwrap();
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn exit_is_not_catchable() {
    let out = run(r#"try { exit; } catch e { out("caught"); } out("after");"#).unwrap();
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn exit_inside_a_function_stops_the_whole_program() {
    let out = run(r#"func f() { exit; } out("before"); f(); out("after");"#).unwrap();
    assert_eq!(out, ["before"]);
}

/// Source-nested literals hit the *parser's* depth guard long before
/// max_value_depth, because one bracket costs more than one parse level. The
/// value-depth limit is only reachable by building the value at runtime - so
/// the two limits guard genuinely different vectors and both are load-bearing.
#[test]
fn parse_depth_and_value_depth_guard_different_vectors() {
    let deep_literal = format!("var x = {}1{};", "[".repeat(80), "]".repeat(80));
    assert!(err(&deep_literal).contains("nested too deeply"));

    let c = cfg(|c| c.max_value_depth = 8);
    assert!(err_with(c, "var a = [1]; while true { a = [a]; }").contains("nested deeper"));
}

/// Errors in included code are labelled, and keep their own line numbers
/// rather than being shifted by whatever was included before them.
#[test]
fn included_code_errors_are_labelled_and_keep_their_own_positions() {
    let (mut nib, _log) = harness(Config::default());
    nib.include("var ok = 1;\nvar bad = ;");
    nib.parse("out(1);").unwrap();
    let e = nib.run().unwrap_err().to_string();
    assert!(e.contains("(in included code)"), "{}", e);
    assert!(e.contains("2:11"), "{}", e);
}
