//! The public API surface itself - `lib.rs`.

#[path = "common/mod.rs"]
mod common;

use common::harness;
use nib_core::{Config, Error, Nib, Value};

#[test]
fn config_is_cloneable_and_comparable() {
    let a = Config::default();
    let mut b = a.clone();
    assert_eq!(a, b);

    b.max_steps = 1;
    assert_ne!(a, b);
    assert!(format!("{:?}", b).contains("max_steps: 1"));
}

#[test]
fn config_defaults_are_the_documented_sandbox_values() {
    let c = Config::default();
    assert_eq!(c.max_call_depth, 200);
    assert_eq!(c.max_parse_depth, 128);
    assert_eq!(c.max_steps, 100_000);
    assert_eq!(c.max_string_length, 65_536);
    assert_eq!(c.max_array_length, 10_000);
    assert_eq!(c.max_map_size, 10_000);
    assert_eq!(c.max_value_depth, 64);
    assert_eq!(c.max_value_nodes, 100_000);
}

/// `parse`/`run`/`disable_keywords` all return `Result<_, Error>`, so `Error`
/// has to be nameable and matchable by an embedder - not just stringifiable.
#[test]
fn error_variants_are_matchable() {
    assert!(matches!(Nib::new().run(), Err(Error::NotParsed)));

    let mut nib = Nib::new();
    match nib.disable_keywords(vec!["Whlie"]) {
        Err(Error::UnknownKeyword(name)) => assert_eq!(name, "Whlie"),
        other => panic!("expected UnknownKeyword, got {:?}", other),
    }

    assert!(matches!(nib.parse("var x = ;"), Err(Error::Parse(_))));
    assert!(matches!(
        nib.parse(r#"var s = "unterminated;"#),
        Err(Error::Lex(_))
    ));

    nib.parse("var x = 1 / 0;").unwrap();
    assert!(matches!(nib.run(), Err(Error::Runtime(_))));
}

#[test]
fn errors_from_included_code_are_distinguishable_from_the_main_script() {
    let (mut nib, _log) = harness(Config::default());
    nib.include("var bad = 1 / 0;");
    nib.parse("out(1);").unwrap();
    assert!(matches!(nib.run(), Err(Error::Included(_))));
}

#[test]
fn error_display_shapes_are_stable() {
    let mut nib = Nib::new();
    assert_eq!(
        Nib::new().run().unwrap_err().to_string(),
        "no script to run: call parse() before run()"
    );
    assert_eq!(
        nib.disable_keywords(vec!["nope"]).unwrap_err().to_string(),
        "cannot disable 'nope': not a nib keyword"
    );
    assert!(
        nib.parse("var x = ;")
            .unwrap_err()
            .to_string()
            .starts_with("Parse error at 1:9:")
    );
}

/// A rejected `disable_keywords` call must apply nothing at all - it validates
/// every name before recording any, so a typo can't silently half-restrict.
#[test]
fn disable_keywords_is_all_or_nothing() {
    let mut nib = Nib::new();
    assert!(nib.disable_keywords(vec!["while", "Whlie"]).is_err());
    // `while` was in the rejected list but must still work
    nib.parse("var i = 0; while i < 1 { i = i + 1; }").unwrap();

    assert!(nib.disable_keywords(vec!["while"]).is_ok());
    assert!(nib.parse("while true { }").is_err());
}

/// `disable_keywords` restricts untrusted *user* scripts; host-chosen
/// `include()` source is the same trust level as a registered native, so it is
/// lexed ignoring the restriction.
#[test]
fn disabled_keywords_do_not_apply_to_included_source() {
    let (mut nib, log) = harness(Config::default());
    nib.disable_keywords(vec!["while"]).unwrap();
    nib.include("func count() { var i = 0; while i < 3 { i = i + 1; } return i; }");
    nib.parse("out(count());").unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["3"]);
}

#[test]
fn native_functions_are_ordinary_values() {
    let (mut nib, log) = harness(Config::default());
    nib.register_func("twice", |args: &[Value]| match args {
        [Value::Int(n)] => Ok(Value::Int(n * 2)),
        _ => Err("twice expects one int".to_string()),
    });
    nib.parse(
        r#"var f = twice;          // can be bound
           out(f(21));
           func apply(g, v) { return g(v); }
           out(apply(twice, 5));   // and passed
           try { twice("x"); } catch e { out(e); }"#,
    )
    .unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["42", "10", "twice expects one int"]);
}

#[test]
fn ast_is_available_after_parse_and_absent_before() {
    let mut nib = Nib::new();
    assert!(nib.ast().is_none());
    nib.parse("var x = 1 + 2;").unwrap();
    assert!(format!("{:?}", nib.ast().unwrap()).contains("Binary"));
}

#[test]
fn nib_implements_default() {
    let mut nib = Nib::default();
    nib.parse("var x = 1;").unwrap();
    assert!(nib.run().is_ok());
}
