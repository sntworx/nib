//! Every `Config` limit - `runtime/interpreter/limits.rs` and `src/types.rs`.
//!
//! Limits are set low via `Config` rather than exercised at their defaults:
//! it keeps the tests fast, and for `max_call_depth` it avoids recursing deep
//! enough to overflow a debug build's stack before the limit reports.

use crate::common::{cfg, err_with, run_stacked, run_with};

#[test]
fn call_depth() {
    let c = cfg(|c| c.max_call_depth = 20);
    let e = err_with(c, "func f() { return f(); } f();");
    assert!(e.contains("maximum call depth of 20"), "{}", e);
}

#[test]
fn call_depth_error_is_recoverable() {
    // the counter is decremented before the error propagates, so a script can
    // keep running afterwards - unlike the step limit below
    let c = cfg(|c| c.max_call_depth = 20);
    let out = run_with(
        c,
        r#"func f() { return f(); } try { f(); } catch e { out("caught"); } out("alive");"#,
    )
    .unwrap();
    assert_eq!(out, ["caught", "alive"]);
}

#[test]
fn parse_depth() {
    let c = cfg(|c| c.max_parse_depth = 16);
    let src = format!("var x = {}1{};", "(".repeat(40), ")".repeat(40));
    assert!(err_with(c, &src).contains("nested too deeply"));
}

#[test]
fn steps() {
    let c = cfg(|c| c.max_steps = 100);
    assert!(err_with(c, "while true { }").contains("maximum execution steps of 100"));
}

#[test]
fn steps_bounds_empty_loop_constructs() {
    // an empty body never reaches exec(), so the loop construct itself ticks
    let c = cfg(|c| c.max_steps = 100);
    assert!(err_with(c.clone(), "for (;;) { }").contains("execution steps"));
    assert!(err_with(c, "while true { }").contains("execution steps"));
}

#[test]
fn steps_budget_is_per_run_not_per_lifetime() {
    let c = cfg(|c| c.max_steps = 50);
    let (mut nib, log) = crate::common::harness(c);
    nib.parse("var i = 0; while i < 5 { i++; } out(i);")
        .unwrap();
    nib.run().unwrap();
    nib.run().unwrap(); // same budget again, not a cumulative total
    assert_eq!(*log.borrow(), ["5", "5"]);
}

#[test]
fn string_length() {
    let c = cfg(|c| c.max_string_length = 16);
    let e = err_with(c, r#"var s = "x"; while true { s = s + s; }"#);
    assert!(e.contains("maximum length of 16 characters"), "{}", e);
}

#[test]
fn string_length_counts_chars_not_bytes() {
    // 6 chars / 16 bytes: allowed at a limit of 6, which byte-counting would reject
    let c = cfg(|c| c.max_string_length = 6);
    let out = run_with(c, r#"out("日本語です!".len());"#).unwrap();
    assert_eq!(out, ["6"]);
}

#[test]
fn array_length() {
    let c = cfg(|c| c.max_array_length = 8);
    let e = err_with(c, "var a = []; while true { a.push(1); }");
    assert!(e.contains("maximum length of 8 elements"), "{}", e);
}

#[test]
fn map_size() {
    let c = cfg(|c| c.max_map_size = 8);
    let e = err_with(
        c,
        r#"var m = {}; var i = 0; while true { m[i.to_str()] = 1; i++; }"#,
    );
    assert!(e.contains("maximum size of 8 entries"), "{}", e);
}

#[test]
fn overwriting_an_existing_key_never_trips_map_size() {
    let c = cfg(|c| c.max_map_size = 1);
    let out = run_with(c, r#"var m = {a: 1}; m["a"] = 2; m["a"] = 3; out(m);"#).unwrap();
    assert_eq!(out, ["{a: 3}"]);
}

#[test]
fn value_depth() {
    let c = cfg(|c| c.max_value_depth = 8);
    let e = err_with(c, "var a = [1]; while true { a = [a]; }");
    assert!(e.contains("nested deeper than 8 levels"), "{}", e);
}

#[test]
fn value_nodes() {
    let c = cfg(|c| c.max_value_nodes = 64);
    let e = err_with(c, "var a = [1]; while true { a = [a, a]; }");
    assert!(e.contains("maximum total size of 64 elements"), "{}", e);
}

/// The documented guarantee: `max_call_depth`'s default must report an error rather
/// than overflow a 1 MiB stack (wasm32's default, and small worker threads).
/// Only meaningful in release - debug frames are far fatter - so it is
/// ignored by default. Run with `cargo test --release -- --ignored`.
#[test]
#[ignore = "release-only: verifies the default max_call_depth against a 1 MiB stack"]
fn default_call_depth_is_safe_on_a_small_stack() {
    let e = run_stacked(1024 * 1024, "func f() { return f(); } f();").unwrap_err();
    assert!(e.contains("maximum call depth"), "{}", e);
}

// --- the second enforcement path ------------------------------------------
//
// Size limits are enforced twice over, by different code: growth in place
// (`push`, `a[i] = x`) is *pre*-checked, because after `Rc::make_mut` has
// mutated there is no pre-mutation value to roll back to; everything that
// builds a fresh value instead is checked afterwards by `check_size_limits`.
// The tests above exercise the pre-check path, these the other one.

#[test]
fn array_literals_are_checked_when_constructed() {
    let c = cfg(|c| c.max_array_length = 3);
    let e = err_with(c, "var a = [1, 2, 3, 4];");
    assert!(e.contains("maximum length of 3 elements"), "{}", e);
}

#[test]
fn map_literals_are_checked_when_constructed() {
    let c = cfg(|c| c.max_map_size = 2);
    let e = err_with(c, "var m = {a: 1, b: 2, c: 3};");
    assert!(e.contains("maximum size of 2 entries"), "{}", e);
}

#[test]
fn string_literals_are_checked_when_constructed() {
    let c = cfg(|c| c.max_string_length = 3);
    assert!(err_with(c, r#"var s = "abcd";"#).contains("maximum length of 3 characters"));
}

/// Pure methods that build a fresh collection are checked on their result,
/// not on their receiver.
#[test]
fn collections_returned_by_methods_are_checked() {
    let c = cfg(|c| c.max_array_length = 3);
    assert!(err_with(c, r#"out("abcd".chars());"#).contains("maximum length of 3 elements"));

    let c = cfg(|c| {
        c.max_array_length = 3;
        c.max_map_size = 10;
    });
    assert!(
        err_with(c.clone(), "out({a: 1, b: 2, c: 3, d: 4}.keys());")
            .contains("maximum length of 3")
    );
    assert!(err_with(c, "out({a: 1, b: 2, c: 3, d: 4}.values());").contains("maximum length of 3"));
}

/// String concatenation grows a value without any container involved.
#[test]
fn concatenation_is_checked_on_every_arm() {
    let c = cfg(|c| c.max_string_length = 5);
    assert!(err_with(c.clone(), r#"out("abc" + "def");"#).contains("maximum length of 5"));
    assert!(err_with(c.clone(), r#"out("abcde" + 1);"#).contains("maximum length of 5"));
    assert!(err_with(c, r#"out(1 + "abcde");"#).contains("maximum length of 5"));
}

/// The node budget has its own pre-check on the in-place paths, separate from
/// the `check_size_limits` arm the literal tests above reach.
#[test]
fn node_budget_is_pre_checked_on_in_place_growth() {
    let c = cfg(|c| c.max_value_nodes = 50);
    assert!(
        err_with(c.clone(), "var a = [1]; while true { a.push(a); }")
            .contains("maximum total size of 50 elements")
    );
    assert!(
        err_with(c, "var a = [1, 2]; while true { a[0] = a; }")
            .contains("maximum total size of 50 elements")
    );
}

/// So does the depth budget.
#[test]
fn depth_budget_is_pre_checked_on_in_place_growth() {
    let c = cfg(|c| c.max_value_depth = 5);
    assert!(
        err_with(c.clone(), "var a = [1]; while true { a.push(a); }")
            .contains("nested deeper than 5 levels")
    );
    assert!(
        err_with(c, "var a = [1, 2]; while true { a[0] = a; }")
            .contains("nested deeper than 5 levels")
    );
}

/// `check_push_room` runs before the method is dispatched, so a `push` on a
/// non-array passes through it and fails later with "no method".
#[test]
fn push_room_check_passes_through_non_arrays() {
    let c = cfg(|c| c.max_array_length = 1);
    assert!(err_with(c, "var m = {a: 1}; m.push(2);").contains("no method"));
}

#[test]
fn map_index_assignment_is_node_budget_checked() {
    let c = cfg(|c| c.max_value_nodes = 50);
    let e = err_with(c, r#"var m = {k: [1, 2]}; while true { m["k"] = m; }"#);
    assert!(e.contains("maximum total size of 50 elements"), "{}", e);
}
