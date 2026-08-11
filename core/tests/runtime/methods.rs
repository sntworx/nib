//! The closed pseudo-method set - `runtime/types/methods.rs`.

use crate::common::{err, run};

#[test]
fn array_methods() {
    let src = r#"let a = [1, 2, 3];
                 out(a.len());
                 out(a.push(4), a.len());
                 out(a.pop(), a.len(), a);"#;
    assert_eq!(run(src).unwrap(), ["3", "[1, 2, 3, 4] 4", "4 3 [1, 2, 3]"]);
}

#[test]
fn map_methods() {
    let src = r#"let m = {a: 1, b: 2};
                 out(m.len(), m.has("a"), m.has("z"));
                 out(m.get("a"), m.get("z"));
                 out(m.remove("a"), m.len(), m);
                 out(m.keys(), m.values());"#;
    assert_eq!(
        run(src).unwrap(),
        ["2 true false", "1 null", "1 1 {b: 2}", "[b] [2]"]
    );
}

/// `get` is the forgiving counterpart to `m[key]`: `Null` instead of an error.
#[test]
fn map_get_is_forgiving_where_indexing_is_not() {
    assert_eq!(run(r#"let m = {}; out(m.get("x"));"#).unwrap(), ["null"]);
    assert!(err(r#"let m = {}; out(m["x"]);"#).len() > 3);
}

#[test]
fn string_methods() {
    let src = r#"let s = "  Hello World  ";
                 out(s.trim().len(), s.trim().upper(), s.trim().lower());
                 out("abc".chars(), "abc".len());
                 out("42".to_int(), "3.5".to_float());"#;
    assert_eq!(
        run(src).unwrap(),
        ["11 HELLO WORLD hello world", "[a, b, c] 3", "42 3.5"]
    );
}

/// `len()` counts characters, not bytes - silently-wrong length would be worse
/// than the O(n) cost.
#[test]
fn string_length_and_chars_are_unicode_aware() {
    let out = run(r#"out("日本語".len(), "日本語".chars(), "ß".upper());"#).unwrap();
    assert_eq!(out, ["3 [日, 本, 語] SS"]);
}

#[test]
fn float_methods() {
    let src = r#"out((3.7).floor(), (3.2).ceil(), (3.5).round(), (2.4).round());
                 out((3.9).to_int(), (-3.9).to_int(), (2.5).to_str());"#;
    assert_eq!(run(src).unwrap(), ["3 4 4 2", "3 -3 2.5"]);
}

/// floor/ceil/round return `Int`, not a `Float` with a zeroed fraction - the
/// usual reason to want the conversion is to index an array.
#[test]
fn rounding_methods_return_ints_usable_as_indices() {
    let out =
        run("let a = [10, 20, 30]; out(a[(2.7).floor()], a[(0.2).ceil()], a[(0.4).round()]);")
            .unwrap();
    assert_eq!(out, ["30 20 10"]);
}

#[test]
fn int_methods() {
    assert_eq!(run("out((7).to_float(), (7).to_str());").unwrap(), ["7 7"]);
}

// --- failures -------------------------------------------------------------

#[test]
fn unknown_methods_are_runtime_errors() {
    assert!(err("out([1].nope());").contains("no method"));
    assert!(err(r#"out("s".nope());"#).contains("no method"));
    // methods are per-type, not shared
    assert!(err(r#"out([1].upper());"#).contains("no method"));
    assert!(err("out((1).floor());").contains("no method"));
}

/// Every method validates its own arity - there is no shared check, so each
/// arm needs its own guard and each guard needs its own test.
#[test]
fn every_method_checks_its_arity() {
    // zero-argument methods, called with one
    for (recv, name) in [
        ("[1]", "len"),
        ("[1]", "pop"),
        ("{a: 1}", "len"),
        ("{a: 1}", "keys"),
        ("{a: 1}", "values"),
        (r#""ab""#, "len"),
        (r#""ab""#, "chars"),
        (r#""ab""#, "upper"),
        (r#""ab""#, "lower"),
        (r#""ab""#, "trim"),
        (r#""12""#, "to_int"),
        (r#""1.5""#, "to_float"),
        ("(1.5)", "floor"),
        ("(1.5)", "ceil"),
        ("(1.5)", "round"),
        ("(1.5)", "to_int"),
        ("(1.5)", "to_str"),
        ("(1)", "to_float"),
        ("(1)", "to_str"),
    ] {
        let src = format!("out({}.{}(99));", recv, name);
        let e = err(&src);
        assert!(
            e.contains(&format!("'{}' expects 0 arguments", name)),
            "{} gave: {}",
            src,
            e
        );
    }

    // one-argument methods, called with none and with two
    for (recv, name) in [
        ("[1]", "push"),
        ("{a: 1}", "has"),
        ("{a: 1}", "get"),
        ("{a: 1}", "remove"),
    ] {
        for args in ["", "1, 2"] {
            let src = format!("out({}.{}({}));", recv, name, args);
            let e = err(&src);
            assert!(
                e.contains(&format!("'{}' expects 1 argument", name)),
                "{} gave: {}",
                src,
                e
            );
        }
    }
}

#[test]
fn mutating_methods_fail_on_an_empty_or_missing_target() {
    assert!(err("let a = []; a.pop();").len() > 3);
    assert!(err(r#"let m = {}; m.remove("x");"#).len() > 3);
}

#[test]
fn typecasting_failures_do_not_echo_the_input() {
    let e = err(r#"out("abc".to_int());"#);
    assert!(e.contains("cannot convert"), "{}", e);
    assert!(
        !e.contains("abc"),
        "script-controlled text leaked into the message: {}",
        e
    );
}

/// Pure methods work on any expression; mutating ones need somewhere to write
/// the receiver back to.
#[test]
fn mutating_methods_require_an_lvalue_but_pure_ones_do_not() {
    assert_eq!(
        run("func f() { return [1, 2]; } out(f().len());").unwrap(),
        ["2"]
    );
    assert!(err("func f() { return [1]; } f().push(2);").contains("invalid assignment target"));
}

/// The map methods that take a key also check its *type*, separately from
/// their arity.
#[test]
fn map_key_methods_reject_non_string_keys() {
    for name in ["has", "get", "remove"] {
        let src = format!("out({{a: 1}}.{}(7));", name);
        let e = err(&src);
        assert!(
            e.contains(&format!("'{}' expects a string argument", name)),
            "{} gave: {}",
            src,
            e
        );
    }
}
