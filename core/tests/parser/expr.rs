//! Expression grammar: precedence, associativity, and the placement rules
//! that fall out of the descent chain - `ast/parser/expr.rs`.

use crate::common::{err, run};

// --- precedence -----------------------------------------------------------

#[test]
fn arithmetic_precedence_and_grouping() {
    let out = run("out(1 + 2 * 3, (1 + 2) * 3, 10 - 2 - 3, 2 * 3 % 4);").unwrap();
    assert_eq!(out, ["7 9 5 2"]);
}

#[test]
fn unary_binds_tighter_than_binary_but_looser_than_postfix() {
    let out = run("out(-2 + 3, !false && false, -(2 + 3));").unwrap();
    assert_eq!(out, ["1 false -5"]);
}

/// The documented gotcha: `.method()` binds tighter than unary `-`, so
/// `-3.7.floor()` is `-(3.7.floor())`, not `(-3.7).floor()`.
#[test]
fn postfix_binds_tighter_than_unary_minus() {
    let out = run("out(-3.7.floor(), (-3.7).floor());").unwrap();
    assert_eq!(out, ["-3 -4"]);
}

#[test]
fn comparison_binds_looser_than_arithmetic() {
    let out = run("out(1 + 1 == 2, 3 > 1 + 1, 1 < 2 == true);").unwrap();
    assert_eq!(out, ["true true true"]);
}

#[test]
fn and_binds_tighter_than_or() {
    // `false && false || true` is `(false && false) || true`
    let out = run("out(false && false || true, true || true && false);").unwrap();
    assert_eq!(out, ["true true"]);
}

#[test]
fn logical_operators_short_circuit() {
    // boom() is undefined - evaluating it would error
    let out = run(r#"out(false && boom(), true || boom());"#).unwrap();
    assert_eq!(out, ["false true"]);
}

// --- ternary --------------------------------------------------------------

#[test]
fn ternary_basics_and_laziness() {
    let out = run(r#"out(1 > 0 ? "y" : "n", 1 < 0 ? boom() : "safe");"#).unwrap();
    assert_eq!(out, ["y safe"]);
}

#[test]
fn ternary_chains_are_right_associative() {
    let src = r#"func g(s) { return s > 90 ? "A" : s > 80 ? "B" : s > 70 ? "C" : "F"; }
                 out(g(95), g(85), g(75), g(10));"#;
    assert_eq!(run(src).unwrap(), ["A B C F"]);
}

#[test]
fn ternary_binds_looser_than_the_binary_operators() {
    // `a || b ? x : y` is `(a || b) ? x : y`
    let out = run(r#"out(false || true ? "t" : "f", 1 + 1 == 2 ? "t" : "f");"#).unwrap();
    assert_eq!(out, ["t t"]);
}

#[test]
fn ternary_nests_inside_collections_and_calls() {
    let src = r#"let s = 95;
                 let m = {grade: s > 90 ? "A" : "B"};
                 let a = [s > 90 ? 1 : 2, 3];
                 out(m["grade"], a[0], s > 90 ? {k: 1} : {k: 2});"#;
    assert_eq!(run(src).unwrap(), ["A 1 {k: 1}"]);
}

#[test]
fn ternary_requires_its_colon() {
    assert!(err("out(1 ? 2);").contains("':'"));
    assert!(err("out(1 ? : 2);").len() > 3);
}

// --- assignment forms -----------------------------------------------------

#[test]
fn compound_assignment_covers_every_arithmetic_operator() {
    let out = run("let y = 5; y += 2; y -= 1; y *= 3; y /= 2; y %= 5; out(y);").unwrap();
    assert_eq!(out, ["4"]);
}

#[test]
fn increment_and_decrement_in_both_positions() {
    let out = run("let x = 1; x++; out(x); ++x; out(x); x--; out(x); --x; out(x);").unwrap();
    assert_eq!(out, ["2", "3", "2", "1"]);
}

/// `++`/`--` and compound assignment parse only at `assignment()` level, which
/// is what `expression()` enters. So they are rejected as a bare operand of a
/// binary/unary operator (those descend *below* assignment)...
#[test]
fn increment_is_rejected_as_a_bare_binary_operand() {
    assert!(err("let x = 1; out(1 + x++);").contains("invalid increment/decrement target"));
    assert!(err("let x = 1; out(1 + x += 1);").contains("invalid assignment target"));
}

/// ...but accepted anywhere `expression()` is the entry point: grouping
/// parens, array elements, map values and call arguments all re-enter the
/// chain at the top.
#[test]
fn assignment_forms_embed_wherever_expression_is_re_entered() {
    assert_eq!(run("let x = 1; out(1 + (x++), x);").unwrap(), ["3 2"]);
    assert_eq!(run("let y = 1; out(1 + (++y), y);").unwrap(), ["3 2"]);
    assert_eq!(run("let z = 1; out([z += 5], z);").unwrap(), ["[6] 6"]);
    assert_eq!(run("let w = 1; out({k: (w += 1)});").unwrap(), ["{k: 2}"]);
    assert_eq!(run("let v = 1; out(v += 1);").unwrap(), ["2"]);
}

/// Prefix forms parse their target at postfix precedence - so `++x` binds to
/// `x` alone, never `x + 1` - and then return immediately. The consequence is
/// that `++x + 1` doesn't parse at all: the trailing `+ 1` is left unconsumed
/// rather than grouping as `(++x) + 1`.
#[test]
fn prefix_increment_takes_only_its_target_and_ends_the_expression() {
    assert!(err("let x = 1; out(++x + 1);").len() > 3);
    // parenthesising closes the assignment expression, after which an operator
    // continues normally
    assert_eq!(run("let x = 1; out((++x) + 1, x);").unwrap(), ["3 2"]);
    assert_eq!(run("let x = 1; out((x++) + 1, x);").unwrap(), ["3 2"]);
    assert_eq!(run("let x = 1; out(++x, x);").unwrap(), ["2 2"]);
}

#[test]
fn assignment_is_right_associative_and_targets_are_checked() {
    assert_eq!(
        run("let a = 0; let b = 0; a = b = 3; out(a, b);").unwrap(),
        ["3 3"]
    );
    assert!(err("1 = 2;").contains("invalid assignment target"));
    assert!(err("let x = 1; (x ? x : x) = 5;").contains("invalid assignment target"));
}

/// There's no declare-and-reassign form beyond `let`.
#[test]
fn assigning_an_undeclared_identifier_fails() {
    assert!(err("undeclared = 1;").len() > 3);
}

// --- literals and access --------------------------------------------------

#[test]
fn array_and_map_literals_including_trailing_content() {
    let out = run(r#"out([], [1], [1, [2]], {}, {a: 1}, {"two words": 2});"#).unwrap();
    assert_eq!(out, ["[] [1] [1, [2]] {} {a: 1} {two words: 2}"]);
}

/// Map keys are static - a string literal or a bare identifier, never an
/// arbitrary expression.
#[test]
fn map_keys_must_be_static() {
    assert!(err("let k = \"a\"; let m = {k + \"b\": 1};").len() > 3);
    assert_eq!(run(r#"let m = {a: 1}; out(m["a"]);"#).unwrap(), ["1"]);
}

#[test]
fn indexing_and_calls_chain() {
    let src = r#"func f() { return [[1, 2]]; }
                 let m = {a: {b: [9]}};
                 out(f()[0][1], m["a"]["b"][0], "ab".upper().len());"#;
    assert_eq!(run(src).unwrap(), ["2 9 2"]);
}

#[test]
fn deeply_nested_expressions_hit_the_parse_depth_guard() {
    let src = format!("let x = {}1{};", "(".repeat(400), ")".repeat(400));
    assert!(err(&src).contains("nested too deeply"));
    // ternaries nest through expression() too, so they're guarded as well
    let ternaries = format!("out({}0{});", "1 ? ".repeat(400), " : 0".repeat(400));
    assert!(err(&ternaries).contains("nested too deeply"));
}
