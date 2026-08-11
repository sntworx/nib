//! Truthiness coercion - `helpers::is_truthy` and its six call sites.

use crate::common::{err, run};

/// Falsy: false, null, zero, empty string, empty array, empty map. Everything
/// else is truthy - including "0" (unlike PHP) and [0].
#[test]
fn falsy_table() {
    let src = r#"let cases = [false, true, null, 0, 1, -1, 0.0, 0.5, "", "0", "a", [], [0], {}, {a: 1}];
                 for c in cases { if c { out("T"); } else { out("F"); } }"#;
    assert_eq!(run(src).unwrap().join(""), "FTFFTTFTFTTFTFT");
}

#[test]
fn every_condition_site_coerces() {
    let src = r#"if 5 { out("if"); }
                 let n = 2; while n { n--; } out(n);
                 for (let s = "ab"; s.len(); s = "") { out("for"); }
                 out(3 ? "ternary" : "no");"#;
    assert_eq!(run(src).unwrap(), ["if", "0", "for", "ternary"]);
}

#[test]
fn logical_operators_coerce_but_return_bools() {
    let out = run(r#"out("a" && "b", "" || "x", null || 5, 0 && 1, [] || {});"#).unwrap();
    assert_eq!(out, ["true true true false false"]);
}

/// No JS-style `x || "default"` idiom: the result is a real Bool, never the
/// deciding operand.
#[test]
fn logical_operators_never_yield_an_operand() {
    assert_eq!(
        run(r#"let v = null || "fallback"; out(v);"#).unwrap(),
        ["true"]
    );
}

#[test]
fn not_applies_to_every_type() {
    let out = run(r#"out(!0, !"", ![], !{}, !null, !false);"#).unwrap();
    assert_eq!(out, ["true true true true true true"]);
    let out = run(r#"out(!1, !"a", ![0], !{a: 1}, !true);"#).unwrap();
    assert_eq!(out, ["false false false false false"]);
}

#[test]
fn short_circuiting_is_preserved() {
    // boom() is undefined, so evaluating it would error
    assert_eq!(
        run("out(0 && boom(), 1 || boom());").unwrap(),
        ["false true"]
    );
}

/// Coercion is confined to boolean *context*: comparisons and arithmetic stay
/// strict, and `match` uses `==` semantics rather than truthiness.
#[test]
fn coercion_does_not_leak_into_equality_or_match() {
    assert_eq!(
        run(r#"out(0 == false, "" == false, [] == false);"#).unwrap(),
        ["false false false"]
    );
    assert_eq!(
        run(r#"match 0 { case false { out("leaked"); } default { out("strict"); } }"#).unwrap(),
        ["strict"]
    );
    assert!(err(r#"out("" < 1);"#).len() > 3);
    assert!(err(r#"out(true + 1);"#).len() > 3);
}

/// There is deliberately no `.to_bool()` - boolean context already converts.
#[test]
fn there_is_no_to_bool_method() {
    assert!(err("out((1).to_bool());").contains("no method"));
    assert!(err(r#"out("x".to_bool());"#).contains("no method"));
}

/// Functions are always truthy - they're values like any other.
#[test]
fn functions_are_truthy() {
    let out = run(r#"func f() { } if f { out("fn truthy"); } out(!f);"#).unwrap();
    assert_eq!(out, ["fn truthy", "false"]);
}
