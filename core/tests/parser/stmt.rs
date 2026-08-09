//! Statement grammar and the rules enforced at parse time -
//! `ast/parser/stmt.rs`.

use crate::common::{err, run};

// --- shapes ---------------------------------------------------------------

#[test]
fn if_else_chain() {
    let src = r#"func f(x) { if x > 2 { return "gt"; } else if x == 2 { return "eq"; } else { return "lt"; } }
                 out(f(3), f(2), f(1));"#;
    assert_eq!(run(src).unwrap(), ["gt eq lt"]);
}

/// `if`/`while`/`match` take a bare expression; parens still work because
/// they're just a grouping expression.
#[test]
fn conditions_take_bare_expressions_but_tolerate_parens() {
    assert_eq!(run("if true { out(1); }").unwrap(), ["1"]);
    assert_eq!(run("if (true) { out(1); }").unwrap(), ["1"]);
    assert_eq!(
        run("var i = 0; while (i < 1) { i++; } out(i);").unwrap(),
        ["1"]
    );
}

/// `for`'s parens are mandatory - they're the only thing delimiting the three
/// clauses - and each clause is optional.
#[test]
fn c_style_for_requires_parens_and_allows_empty_clauses() {
    assert!(err("for var i = 0; i < 1; i++ { }").len() > 3);
    assert_eq!(
        run("for (var i = 0; i < 3; i++) { out(i); }").unwrap(),
        ["0", "1", "2"]
    );
    assert_eq!(
        run("var i = 0; for (; i < 2;) { i++; } out(i);").unwrap(),
        ["2"]
    );
    assert_eq!(
        run("var n = 0; for (;;) { n++; if n > 2 { break; } } out(n);").unwrap(),
        ["3"]
    );
}

#[test]
fn for_in_never_takes_parens() {
    assert_eq!(run("for x in [1, 2] { out(x); }").unwrap(), ["1", "2"]);
    // a parenthesised subject is still just a grouping expression
    assert_eq!(run("for x in ([1]) { out(x); }").unwrap(), ["1"]);
}

#[test]
fn bare_block_is_its_own_statement_and_scope() {
    assert_eq!(run("{ var inner = 1; out(inner); }").unwrap(), ["1"]);
    assert!(err("{ var inner = 1; } out(inner);").len() > 3);
}

#[test]
fn return_takes_an_optional_expression() {
    assert_eq!(
        run("func a() { return; } func b() { return 1; } out(a(), b());").unwrap(),
        ["null 1"]
    );
}

// --- parse-time rules -----------------------------------------------------

/// `func` is top level only, at any nesting depth - one `at_top_level` flag
/// covers every case rather than separate func-in-func/func-in-loop checks.
#[test]
fn func_declarations_are_top_level_only() {
    for src in [
        "func a() { func b() { } }",
        "if true { func b() { } }",
        "while true { func b() { } }",
        "for (;;) { func b() { } }",
        "for x in [1] { func b() { } }",
        "{ func b() { } }",
        "{ { func b() { } } }",
        "match 1 { case 1 { func b() { } } }",
        "try { func b() { } } catch e { }",
    ] {
        assert!(
            err(src).contains("top level"),
            "should be rejected: {}",
            src
        );
    }
}

/// The reverse is fine: loops and blocks *inside* a function are ordinary.
#[test]
fn blocks_and_loops_inside_a_function_are_fine() {
    let src = "func f() { var n = 0; for (var i = 0; i < 3; i++) { if i > 0 { n += i; } } return n; } out(f());";
    assert_eq!(run(src).unwrap(), ["3"]);
}

#[test]
fn break_and_continue_require_an_enclosing_loop() {
    assert!(err("break;").len() > 3);
    assert!(err("continue;").len() > 3);
    assert!(err("if true { break; }").len() > 3);
    assert!(err("func f() { break; }").len() > 3);
    // ...and a func body inside a loop doesn't inherit the loop
    assert!(err("for (;;) { } func f() { continue; }").len() > 3);
}

#[test]
fn match_default_must_be_last() {
    assert!(err("match 1 { default { } case 1 { } }").len() > 3);
    assert_eq!(
        run(r#"match 1 { case 2 { out("a"); } default { out("d"); } }"#).unwrap(),
        ["d"]
    );
}

#[test]
fn try_must_be_paired_with_catch() {
    assert!(err("try { out(1); }").len() > 3);
    assert!(err("try { out(1); } finally { }").len() > 3);
}

/// `catch e { }`, not `catch (e) { }` - matching the bare-binding style
/// `for x in arr` already uses.
#[test]
fn catch_binding_takes_no_parens() {
    assert!(err(r#"try { throw "x"; } catch (e) { }"#).len() > 3);
    assert_eq!(
        run(r#"try { throw "x"; } catch e { out(e); }"#).unwrap(),
        ["x"]
    );
}

/// `for`'s init clause takes any statement, not just a `var` declaration.
#[test]
fn for_init_accepts_a_bare_expression() {
    let out = run("var i = 0; for (i = 0; i < 2; i++) { out(i); } out(i);").unwrap();
    assert_eq!(out, ["0", "1", "2"]);
}

#[test]
fn match_rejects_a_second_default_arm() {
    assert!(err("match 1 { default { } default { } }").contains("one 'default' arm"));
}

#[test]
fn map_keys_reject_non_identifier_literals() {
    assert!(err("var m = {1: 2};").contains("map key"));
    assert!(err("var m = {true: 2};").contains("map key"));
}
