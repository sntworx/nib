//! Tokenization - `lexer/`. Reachable only through `parse()`, so lexical
//! rules are pinned via the values they produce and the errors they raise.

#[path = "common/mod.rs"]
mod common;

use common::{err, run};
use nib_core::{Error, Nib};

#[test]
fn string_escapes() {
    let out = run(r#"out("a\nb"); out("a\tb"); out("a\"b"); out("a\\b");"#).unwrap();
    assert_eq!(out, ["a\nb", "a\tb", "a\"b", "a\\b"]);
}

#[test]
fn unterminated_string_is_a_lex_error() {
    let mut nib = Nib::new();
    assert!(matches!(nib.parse(r#"var s = "oops;"#), Err(Error::Lex(_))));
}

#[test]
fn line_and_block_comments_are_skipped() {
    let out = run(r#"// leading comment
           var x = 1; // trailing
           /* block
              spanning lines */
           out(x); /* inline */ out(2);"#)
    .unwrap();
    assert_eq!(out, ["1", "2"]);
}

/// Block comments deliberately don't nest: the first `*/` closes them.
#[test]
fn block_comments_do_not_nest() {
    let out = run("/* outer /* inner */ out(1);").unwrap();
    assert_eq!(out, ["1"]);
}

#[test]
fn unterminated_block_comment_is_a_lex_error() {
    let mut nib = Nib::new();
    assert!(matches!(
        nib.parse("/* never closed\nout(1);"),
        Err(Error::Lex(_))
    ));
}

#[test]
fn numeric_literals() {
    let out = run("out(0); out(42); out(3.5); out(0.0); out(1000000);").unwrap();
    assert_eq!(out, ["0", "42", "3.5", "0", "1000000"]);
}

/// Out-of-range literals are rejected at lex time rather than silently clamped.
#[test]
fn out_of_range_literals_are_rejected() {
    let mut nib = Nib::new();
    assert!(matches!(
        nib.parse("var x = 99999999999999999999;"),
        Err(Error::Lex(_))
    ));
    assert!(nib.parse("var x = 9223372036854775807;").is_ok());
}

/// A lone `&`/`|` is a common typo, so it gets a pointed message rather than
/// a bare "unexpected character".
#[test]
fn single_ampersand_or_pipe_suggests_the_doubled_form() {
    assert!(err("var x = 1 & 2;").contains("&&"));
    assert!(err("var x = 1 | 2;").contains("||"));
}

/// Diagnostics must not echo raw script-controlled bytes (e.g. terminal
/// escapes) back to the host, so unexpected characters are debug-formatted.
#[test]
fn unexpected_control_characters_are_escaped_in_diagnostics() {
    let e = err("var x = \u{1b};");
    assert!(e.contains("\\u{1b}"), "{}", e);
    assert!(
        !e.contains('\u{1b}'),
        "raw escape byte leaked into the message"
    );
}

#[test]
fn positions_are_reported_as_line_and_column() {
    let e = err("var a = 1;\nvar b = ;");
    assert!(e.contains("2:9"), "{}", e);
}

#[test]
fn keywords_are_not_identifiers() {
    assert!(err("var if = 1;").len() > 3);
    // ...but words merely containing a keyword are fine
    let out = run("var iffy = 1; var format = 2; out(iffy, format);").unwrap();
    assert_eq!(out, ["1 2"]);
}

/// Unknown escapes pass through leniently rather than erroring.
#[test]
fn unknown_escapes_pass_through() {
    let out = run(r#"out("a\qb"); out("\z");"#).unwrap();
    assert_eq!(out, ["a\\qb", "\\z"]);
}

#[test]
fn a_string_ending_mid_escape_is_unterminated() {
    let mut nib = Nib::new();
    assert!(matches!(nib.parse("var s = \"abc\\"), Err(Error::Lex(_))));
}

/// With no exponent form, an out-of-range float needs a very long literal -
/// but the guard is real and rejects it rather than storing `inf`.
#[test]
fn out_of_range_float_literals_are_rejected() {
    let huge = format!("var x = 1{}.0;", "0".repeat(400));
    let mut nib = Nib::new();
    let e = nib.parse(&huge).unwrap_err().to_string();
    assert!(e.contains("out of range"), "{}", e);
}
