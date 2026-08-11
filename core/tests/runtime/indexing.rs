//! Indexing and the copy-on-write write-back machinery -
//! `runtime/interpreter/index.rs`.

use crate::common::{err, run};

#[test]
fn reads_chain_across_arrays_and_maps() {
    let src = r#"let d = {users: [{name: "a"}, {name: "b"}]};
                 out(d["users"][1]["name"]);"#;
    assert_eq!(run(src).unwrap(), ["b"]);
}

#[test]
fn writes_chain_and_write_back_through_every_level() {
    let src = r#"let d = {users: [{name: "a"}]};
                 d["users"][0]["name"] = "z";
                 out(d);"#;
    assert_eq!(run(src).unwrap(), ["{users: [{name: z}]}"]);
}

#[test]
fn compound_and_increment_forms_reuse_the_same_machinery() {
    let src = r#"let a = [1, 2];
                 a[0] += 10; a[1] *= 3; a[0]++; a[1]--;
                 out(a);
                 let m = {n: 1};
                 m["n"] += 5; m["n"]++;
                 out(m);"#;
    assert_eq!(run(src).unwrap(), ["[12, 5]", "{n: 7}"]);
}

#[test]
fn array_indices_must_be_ints_in_range() {
    assert!(err("let a = [1]; out(a[1.0]);").len() > 3);
    assert!(err(r#"let a = [1]; out(a["0"]);"#).len() > 3);
    assert!(err("let a = [1]; out(a[-1]);").len() > 3);
    assert!(err("let a = [1]; out(a[1]);").contains("out of bounds"));
}

#[test]
fn map_keys_must_be_strings() {
    assert!(err("let m = {a: 1}; out(m[0]);").len() > 3);
}

#[test]
fn indexing_a_scalar_is_an_error() {
    for src in ["out((1)[0]);", r#"out(true[0]);"#, "out(null[0]);"] {
        assert!(err(src).contains("cannot index into"), "{}", src);
    }
}

/// Strings bridge to `Array` via `chars()` rather than being indexable.
#[test]
fn strings_are_not_directly_indexable() {
    assert!(err(r#"out("abc"[0]);"#).contains("cannot index into"));
    assert_eq!(run(r#"out("abc".chars()[0]);"#).unwrap(), ["a"]);
}

/// Mutation detaches the value from its binding so `make_mut` can work in
/// place - but the arguments must be evaluated *before* that happens, or
/// `a.push(a.len())` would see a detached `null`.
#[test]
fn arguments_are_evaluated_before_the_receiver_is_detached() {
    assert_eq!(
        run("let a = [1, 2]; a.push(a.len()); out(a);").unwrap(),
        ["[1, 2, 2]"]
    );
    assert_eq!(
        run("let a = [5, 6]; a[0] = a[1]; out(a);").unwrap(),
        ["[6, 6]"]
    );
    assert_eq!(
        run("let a = [1]; a[0] = a.len(); out(a);").unwrap(),
        ["[1]"]
    );
}

/// Self-referential assignment stores a *snapshot*, not a cycle - which is
/// what makes reference counting sufficient and a GC unnecessary.
#[test]
fn self_reference_stores_a_snapshot_rather_than_a_cycle() {
    let out = run("let a = [1]; a.push(a); out(a); a[0] = 9; out(a);").unwrap();
    assert_eq!(out, ["[1, [1]]", "[9, [1]]"]);
}

/// An accepted limitation: nested write-back re-evaluates the outer
/// `object`/`index` sub-expressions, so side effects there run twice.
#[test]
fn nested_write_back_re_evaluates_outer_subexpressions() {
    let src = r#"let calls = 0;
                 func i() { calls = calls + 1; return 0; }
                 let m = [[1, 2]];
                 m[i()][1] = 9;
                 out(m, calls);"#;
    assert_eq!(run(src).unwrap(), ["[[1, 9]] 2"]);
}

// --- write-back failure paths ---------------------------------------------
//
// `with_index_replaced` patches the *outer* levels of a nested assignment, and
// its error arms are only reachable because write-back re-evaluates those
// sub-expressions (see `nested_write_back_re_evaluates_outer_subexpressions`).
// A side-effecting index that changes between the two evaluations is the only
// way in - contrived, but these arms are real and otherwise untested.

#[test]
fn write_back_reports_an_index_that_went_out_of_bounds() {
    let src = r#"let m = [[1, 2]];
                 let n = 0;
                 func idx() { n = n + 1; return n == 1 ? 0 : 5; }
                 m[idx()][0] = 9;"#;
    assert!(err(src).contains("index 5 out of bounds"));
}

#[test]
fn write_back_reports_an_index_that_stopped_being_an_integer() {
    let src = r#"let m = [[1, 2]];
                 let n = 0;
                 func idx() { n = n + 1; return n == 1 ? 0 : "x"; }
                 m[idx()][0] = 9;"#;
    assert!(err(src).contains("array index must be an integer, got string"));
}

#[test]
fn write_back_reports_a_map_key_that_stopped_being_a_string() {
    let src = r#"let m = {a: [1, 2]};
                 let n = 0;
                 func k() { n = n + 1; return n == 1 ? "a" : 7; }
                 m[k()][0] = 9;"#;
    assert!(err(src).contains("map key must be a string, got int"));
}

#[test]
fn write_back_reports_a_container_that_stopped_being_indexable() {
    let src = r#"let m = [[1, 2]];
                 let n = 0;
                 func idx() { n = n + 1; if n == 2 { m = 5; } return 0; }
                 m[idx()][0] = 9;"#;
    assert!(err(src).contains("cannot index into int"));
}

// --- index-assignment type errors -----------------------------------------

#[test]
fn assigning_with_a_wrong_typed_index_is_rejected() {
    assert!(err(r#"let a = [1]; a["x"] = 2;"#).contains("array index must be an integer"));
    assert!(err("let a = [1]; a[1.0] = 2;").contains("array index must be an integer"));
    assert!(err("let m = {a: 1}; m[0] = 2;").contains("map key must be a string"));
    assert!(err("let x = 1; x[0] = 2;").contains("cannot index into int"));
    assert!(err(r#"let s = "ab"; s[0] = "c";"#).contains("cannot index into string"));
}

#[test]
fn compound_assignment_checks_the_index_type_too() {
    assert!(err(r#"let a = [1]; a["x"] += 2;"#).contains("array index must be an integer"));
    assert!(err("let m = {a: 1}; m[0] += 2;").contains("map key must be a string"));
}
