//! Value semantics, equality and Display - `runtime/types/{mod,containers}.rs`.

use crate::common::{err, run};

// --- value, not reference, semantics -------------------------------------

#[test]
fn assignment_copies_arrays() {
    let out = run("let a = [1, 2]; let b = a; b[0] = 99; out(a[0], b[0]);").unwrap();
    assert_eq!(out, ["1 99"]);
}

#[test]
fn assignment_copies_maps() {
    let out = run(r#"let m = {x: 1}; let m2 = m; m2["x"] = 5; out(m["x"], m2["x"]);"#).unwrap();
    assert_eq!(out, ["1 5"]);
}

#[test]
fn copy_survives_mutating_methods() {
    let out = run("let a = [1]; let b = a; a.push(2); out(a.len(), b.len());").unwrap();
    assert_eq!(out, ["2 1"]);
}

#[test]
fn nested_containers_are_copied_too() {
    let out = run("let a = [[1, 2]]; let b = a; b[0][0] = 9; out(a[0][0], b[0][0]);").unwrap();
    assert_eq!(out, ["1 9"]);
}

#[test]
fn for_in_hands_elements_over_by_value() {
    let out = run("let a = [[1]]; for x in a { x[0] = 9; } out(a[0][0]);").unwrap();
    assert_eq!(out, ["1"]);
}

#[test]
fn function_arguments_are_copies() {
    let out =
        run("func f(v) { v.push(2); return v.len(); } let a = [1]; out(f(a), a.len());").unwrap();
    assert_eq!(out, ["2 1"]);
}

#[test]
fn nested_index_assignment_writes_back() {
    let out = run("let m = [[1, 2], [3, 4]]; m[1][0] = 99; out(m);").unwrap();
    assert_eq!(out, ["[[1, 2], [99, 4]]"]);
}

// --- equality -------------------------------------------------------------

#[test]
fn maps_compare_ignoring_insertion_order() {
    let out = run("out({a: 1, b: 2} == {b: 2, a: 1});").unwrap();
    assert_eq!(out, ["true"]);
}

#[test]
fn arrays_compare_respecting_order() {
    let out = run("out([1, 2] == [2, 1], [1, 2] == [1, 2]);").unwrap();
    assert_eq!(out, ["false true"]);
}

#[test]
fn ints_and_floats_compare_numerically() {
    let out = run("out(1 == 1.0, 1 != 1.0, 2 == 2.5);").unwrap();
    assert_eq!(out, ["true false false"]);
}

/// Truthiness is confined to boolean *context*; it must never leak into `==`.
#[test]
fn falsy_values_are_not_equal_to_false() {
    let out =
        run(r#"out(0 == false, "" == false, [] == false, null == false, {} == false);"#).unwrap();
    assert_eq!(out, ["false false false false false"]);
}

#[test]
fn functions_compare_by_identity() {
    let out = run("func f() { } func g() { } let h = f; out(f == h, f == g);").unwrap();
    assert_eq!(out, ["true false"]);
}

#[test]
fn nested_structures_compare_structurally() {
    let out =
        run(r#"out([1, {k: [2]}] == [1, {k: [2]}], [1, {k: [2]}] == [1, {k: [3]}]);"#).unwrap();
    assert_eq!(out, ["true false"]);
}

// --- Display --------------------------------------------------------------

#[test]
fn displays_scalars() {
    let out = run(r#"out(1); out(2.5); out("s"); out(true); out(null);"#).unwrap();
    assert_eq!(out, ["1", "2.5", "s", "true", "null"]);
}

#[test]
fn displays_containers_in_insertion_order() {
    let out = run(r#"out([1, [2, 3]]); out({b: 1, a: 2}); out({k: [1, {n: 2}]});"#).unwrap();
    assert_eq!(out, ["[1, [2, 3]]", "{b: 1, a: 2}", "{k: [1, {n: 2}]}"]);
}

#[test]
fn map_keys_and_values_follow_insertion_order() {
    let out = run(r#"let m = {b: 1, a: 2}; m["c"] = 3; out(m.keys(), m.values());"#).unwrap();
    assert_eq!(out, ["[b, a, c] [1, 2, 3]"]);
}

// --- indexing errors ------------------------------------------------------

#[test]
fn reading_a_missing_map_key_errors() {
    assert!(err(r#"let m = {}; out(m["nope"]);"#).contains("nope"));
}

#[test]
fn array_index_is_bounds_checked_and_cannot_grow() {
    assert!(err("let a = [1]; a[5] = 2;").contains("index 5 out of bounds for array of length 1"));
    assert!(err("let a = [1]; out(a[5]);").contains("index 5 out of bounds for array of length 1"));
}

#[test]
fn map_index_assignment_upserts() {
    let out = run(r#"let m = {a: 1}; m["b"] = 2; m["a"] = 9; out(m);"#).unwrap();
    assert_eq!(out, ["{a: 9, b: 2}"]);
}

#[test]
fn compound_assignment_needs_an_existing_map_key() {
    assert!(err(r#"let m = {}; m["x"] += 1;"#).contains("compound assignment"));
}

#[test]
fn assigning_into_a_temporary_is_rejected() {
    assert!(err("func f() { return [1]; } f()[0] = 2;").contains("invalid assignment target"));
    assert!(err("func f() { return [1]; } f().push(2);").contains("invalid assignment target"));
}

// --- function values ------------------------------------------------------

#[test]
fn functions_display_with_their_name() {
    let out = run("func greet() { } let alias = greet; out(greet, alias);").unwrap();
    assert_eq!(out, ["<function greet> <function greet>"]);
}

#[test]
fn functions_report_their_type_in_errors() {
    assert!(err("func f() { } out(f[0]);").contains("cannot index into function"));
    assert!(err("func f() { } out(f + 1);").contains("function"));
}

#[test]
fn returning_and_comparing_functions() {
    let src = r#"func a() { return 1; }
                 func makeA() { return a; }
                 out(makeA() == a, makeA()());"#;
    assert_eq!(run(src).unwrap(), ["true 1"]);
}
