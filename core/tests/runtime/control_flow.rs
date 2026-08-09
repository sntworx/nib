//! Statement execution and the `Flow` machinery -
//! `runtime/interpreter/exec.rs`.

use crate::common::{cfg, err, run, run_with};

#[test]
fn while_and_c_style_for() {
    let src = r#"var i = 0; while i < 3 { out(i); i++; }
                 for (var j = 2; j > 0; j--) { out(j); }"#;
    assert_eq!(run(src).unwrap(), ["0", "1", "2", "2", "1"]);
}

#[test]
fn break_and_continue_in_both_loop_forms() {
    let src = r#"var acc = 0;
                 for (var i = 0; i < 5; i++) { if i == 1 { continue; } if i == 4 { break; } acc += i; }
                 out(acc);
                 var n = 0;
                 while true { n++; if n < 3 { continue; } break; }
                 out(n);"#;
    assert_eq!(run(src).unwrap(), ["5", "3"]);
}

/// `continue` in a C-style `for` still runs the post clause, matching C/JS.
#[test]
fn continue_runs_the_post_clause() {
    let src = "var seen = []; for (var i = 0; i < 4; i++) { if i == 1 { continue; } seen.push(i); } out(seen);";
    assert_eq!(run(src).unwrap(), ["[0, 2, 3]"]);
}

#[test]
fn break_only_exits_the_innermost_loop() {
    let src = r#"var hits = 0;
                 for (var i = 0; i < 2; i++) { for (var j = 0; j < 5; j++) { if j == 1 { break; } hits++; } }
                 out(hits);"#;
    assert_eq!(run(src).unwrap(), ["2"]);
}

/// The for-in subject is evaluated once up front, like `for`'s init clause -
/// reassigning the source mid-loop doesn't change what's iterated.
#[test]
fn for_in_evaluates_its_subject_once() {
    let src = "var a = [1, 2, 3]; for x in a { a = [9]; out(x); }";
    assert_eq!(run(src).unwrap(), ["1", "2", "3"]);
}

#[test]
fn for_in_requires_an_array() {
    for src in [
        r#"for c in "abc" { }"#,
        "for k in {a: 1} { }",
        "for x in 5 { }",
    ] {
        assert!(err(src).len() > 3, "{} should not iterate", src);
    }
    // strings and maps bridge through methods instead
    assert_eq!(
        run(r#"for c in "ab".chars() { out(c); }"#).unwrap(),
        ["a", "b"]
    );
    assert_eq!(run("for k in {a: 1}.keys() { out(k); }").unwrap(), ["a"]);
}

// --- match ----------------------------------------------------------------

#[test]
fn match_takes_the_first_arm_and_does_not_fall_through() {
    let src = r#"func f(v) { match v { case 1 { return "one"; } case 1 { return "dup"; } default { return "d"; } } }
                 out(f(1), f(9));"#;
    assert_eq!(run(src).unwrap(), ["one d"]);
}

/// Patterns are ordinary expressions compared with `==`, not a binding grammar.
#[test]
fn match_patterns_are_arbitrary_expressions() {
    let src = r#"var x = 2;
                 match 3 { case x + 1 { out("computed"); } default { out("no"); } }
                 match [1, 2] { case [1, 2] { out("structural"); } default { out("no"); } }
                 match 1 { case 1.0 { out("numeric"); } default { out("no"); } }"#;
    assert_eq!(run(src).unwrap(), ["computed", "structural", "numeric"]);
}

#[test]
fn match_without_a_default_can_match_nothing() {
    assert_eq!(
        run(r#"match 9 { case 1 { out("a"); } } out("after");"#).unwrap(),
        ["after"]
    );
}

// --- try / catch / throw --------------------------------------------------

#[test]
fn throw_carries_any_value_through_to_catch() {
    let src = r#"try { throw {code: 42, msg: "bad"}; } catch e { out(e["code"], e["msg"]); }
                 try { throw [1, 2]; } catch e { out(e, e.len()); }
                 try { throw 7; } catch e { out(e + 1); }"#;
    assert_eq!(run(src).unwrap(), ["42 bad", "[1, 2] 2", "8"]);
}

/// Interpreter-raised errors arrive as a plain string, matching the
/// convention native-function errors already use.
#[test]
fn builtin_errors_arrive_as_strings() {
    let out = run(r#"try { var x = 1 / 0; } catch e { out(e); }"#).unwrap();
    assert_eq!(out, ["division by zero"]);
}

/// Only the try block is guarded - an error raised inside catch propagates
/// rather than being caught by its own try.
#[test]
fn catch_blocks_are_not_self_guarding() {
    assert!(err(r#"try { throw "a"; } catch e { var x = 1 / 0; }"#).contains("division by zero"));
}

#[test]
fn try_catch_nests() {
    let src = r#"try { try { throw "inner"; } catch e { out("caught " + e); throw "outer"; } }
                 catch e { out("outer caught " + e); }"#;
    assert_eq!(run(src).unwrap(), ["caught inner", "outer caught outer"]);
}

#[test]
fn errors_propagate_out_of_function_calls_to_an_enclosing_try() {
    let src = r#"func boom() { throw "from f"; }
                 try { boom(); } catch e { out(e); }"#;
    assert_eq!(run(src).unwrap(), ["from f"]);
}

// --- exit -----------------------------------------------------------------

#[test]
fn exit_stops_everything_including_enclosing_loops() {
    let src = r#"for (var i = 0; i < 5; i++) { out(i); if i == 1 { exit; } } out("never");"#;
    assert_eq!(run(src).unwrap(), ["0", "1"]);
}

/// `exit` resets per `run()`, so it stops only the source it appears in.
#[test]
fn exit_is_scoped_to_one_run() {
    let (mut nib, log) = crate::common::harness(cfg(|_| {}));
    nib.include(r#"out("lib start"); exit; out("lib end");"#);
    nib.parse(r#"out("main");"#).unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["lib start", "main"]);
}

#[test]
fn exit_leaves_the_current_statement_to_finish() {
    // control flow is only checked at statement granularity, never mid-expression
    let src = r#"func f() { exit; } out("a"); var x = f(); out("b");"#;
    assert_eq!(run(src).unwrap(), ["a"]);
}

#[test]
fn return_outside_a_function_is_an_error() {
    assert!(err("return 1;").contains("outside of function"));
}

#[test]
fn deeply_nested_blocks_execute() {
    let c = cfg(|c| c.max_steps = 10_000);
    let src = "var n = 0; { { { for (var i = 0; i < 3; i++) { { n += i; } } } } } out(n);";
    assert_eq!(run_with(c, src).unwrap(), ["3"]);
}

#[test]
fn exit_stops_a_for_in_loop() {
    let src = r#"for x in [1, 2, 3] { out(x); if x == 2 { exit; } } out("never");"#;
    assert_eq!(run(src).unwrap(), ["1", "2"]);
}

#[test]
fn break_and_continue_work_in_for_in() {
    let src = r#"var seen = [];
                 for x in [1, 2, 3, 4] { if x == 2 { continue; } if x == 4 { break; } seen.push(x); }
                 out(seen);"#;
    assert_eq!(run(src).unwrap(), ["[1, 3]"]);
}
