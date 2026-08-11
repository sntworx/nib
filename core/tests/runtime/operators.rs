//! Arithmetic, comparison and numeric coercion -
//! `runtime/interpreter/expr.rs` and `runtime/helpers.rs`.

use crate::common::{err, run};

/// `Int op Int` stays `Int`, including truncating division; any other numeric
/// pairing promotes both sides to f64.
#[test]
fn int_arithmetic_stays_int_and_division_truncates() {
    let out = run("out(7 / 2, -7 / 2, 7 % 3, 2 * 3, 5 - 8);").unwrap();
    assert_eq!(out, ["3 -3 1 6 -3"]);
}

#[test]
fn mixed_arithmetic_promotes_to_float() {
    let out = run("out(1 + 1.5, 3.0 / 2, 7.0 % 2.5, 2 * 0.5);").unwrap();
    assert_eq!(out, ["2.5 1.5 2 1"]);
}

/// `%` is remainder with the sign of the dividend (C/JS/Rust), not Python's
/// floored modulo.
#[test]
fn modulo_follows_the_dividend_sign() {
    let out = run("out(-7 % 3, 7 % -3, -7.5 % 2);").unwrap();
    assert_eq!(out, ["-1 1 -1.5"]);
}

#[test]
fn division_and_modulo_by_zero_are_errors_not_infinities() {
    assert!(err("out(1 / 0);").contains("division by zero"));
    assert!(err("out(1 % 0);").contains("modulo by zero"));
    assert!(err("out(1.0 / 0.0);").len() > 3);
}

#[test]
fn float_results_must_stay_finite() {
    let e = err("let big = 1.0; let i = 0; while i < 400 { big = big * 10.0; i++; }");
    assert!(e.contains("overflow"), "{}", e);
}

/// Float literals have no exponent form - `1.0e308` lexes as `1.0` followed by
/// the identifier `e308`, which is a parse error. Large magnitudes have to be
/// built by arithmetic.
#[test]
fn float_literals_have_no_scientific_notation() {
    assert!(err("let big = 1.0e308;").len() > 3);
    assert!(err("let small = 1e-9;").len() > 3);
}

// --- comparison -----------------------------------------------------------

#[test]
fn numeric_comparison_coerces_across_int_and_float() {
    let out = run("out(1 < 1.5, 2.0 == 2, 3 >= 3.0, 2 != 2.0);").unwrap();
    assert_eq!(out, ["true true true false"]);
}

#[test]
fn strings_compare_lexicographically() {
    let out = run(r#"out("a" < "b", "abc" < "abd", "b" > "a", "a" <= "a");"#).unwrap();
    assert_eq!(out, ["true true true true"]);
}

/// Ordering comparisons never coerce across unrelated types - only `==`/`!=`
/// work cross-type (by returning false).
#[test]
fn cross_type_ordering_is_an_error_while_equality_is_not() {
    for src in [
        r#"out("1" < 2);"#,
        "out(true < 2);",
        "out([1] < [2]);",
        "out(null < 1);",
    ] {
        assert!(err(src).len() > 3, "{} should not compare", src);
    }
    assert_eq!(
        run(r#"out("1" == 1, true == 1, null == 0);"#).unwrap(),
        ["false false false"]
    );
}

// --- string concatenation -------------------------------------------------

/// `+` is the one deliberate exception: it stringifies `Int`/`Float` only.
#[test]
fn plus_concatenates_strings_with_numbers_in_either_order() {
    let out = run(r#"out("n: " + 5, 5 + " n", "f: " + 1.5, "a" + "b");"#).unwrap();
    assert_eq!(out, ["n: 5 5 n f: 1.5 ab"]);
}

#[test]
fn plus_does_not_generalise_to_other_types() {
    for src in [
        r#"out("x: " + true);"#,
        r#"out("x: " + null);"#,
        r#"out("x: " + [1]);"#,
        r#"out("x: " + {a: 1});"#,
    ] {
        assert!(err(src).len() > 3, "{} should not concatenate", src);
    }
}

// --- overflow -------------------------------------------------------------

#[test]
fn integer_overflow_errors_on_every_operator() {
    let max = "9223372036854775807";
    assert!(err(&format!("out({} + 1);", max)).contains("integer overflow"));
    assert!(err(&format!("out({} * 2);", max)).contains("integer overflow"));
    assert!(err(&format!("out(-{} - 2);", max)).contains("integer overflow"));
    assert!(err(&format!("let m = -{} - 1; out(-m);", max)).contains("integer overflow"));
}

#[test]
fn unary_operators_reject_inapplicable_types() {
    assert!(err(r#"out(-"a");"#).contains("unary operator"));
    assert!(err("out(-[1]);").contains("unary operator"));
    // `!` applies to every type via truthiness, so it never errors
    assert_eq!(
        run(r#"out(!0, !"", ![], !1, !"a");"#).unwrap(),
        ["true true true false false"]
    );
}

/// Both operand orders of every `Str + number` arm.
#[test]
fn string_concatenation_covers_all_four_arms() {
    let out = run(r#"out("a" + 1, 1 + "a", "a" + 1.5, 1.5 + "a");"#).unwrap();
    assert_eq!(out, ["a1 1a a1.5 1.5a"]);
}

/// Every comparison operator, on ints, floats, mixed pairs and strings.
#[test]
fn every_comparison_operator_on_every_comparable_pairing() {
    let out = run(r#"out(1 < 2, 2 <= 2, 3 > 2, 3 >= 4);
           out(1.5 < 2.5, 2.5 <= 2.5, 3.5 > 2.5, 3.5 >= 4.5);
           out(1 < 2.5, 2 <= 2.0, 3 > 2.5, 3 >= 4.0);
           out(2.5 < 3, 2.0 <= 2, 3.5 > 3, 3.5 >= 4);
           out("a" < "b", "a" <= "a", "b" > "a", "a" >= "b");"#)
    .unwrap();
    assert_eq!(
        out,
        [
            "true true true false",
            "true true true false",
            "true true true false",
            "true true true false",
            "true true true false",
        ]
    );
}

/// Every arithmetic operator in the promoted-float branch.
#[test]
fn float_arithmetic_covers_every_operator() {
    let out = run("out(1.5 + 0.5, 1.5 - 0.25, 1.5 * 2, 3.0 / 1.5, 5.5 % 2);").unwrap();
    assert_eq!(out, ["2 1.25 3 2 1.5"]);
}

#[test]
fn equality_on_every_scalar_type() {
    let out =
        run(r#"out("a" == "a", "a" == "b", null == null, true == true, true == false);"#).unwrap();
    assert_eq!(out, ["true false true true false"]);
}
