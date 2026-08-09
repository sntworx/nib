//! End-to-end tests for the `nib` binary: flags, output streams and exit
//! codes. These drive the real executable, so they cover the argument
//! plumbing and the host natives (`print`/`println`/`read`) that `nib_core`
//! deliberately doesn't provide.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use std::fs;
use tempfile::TempDir;

/// Writes `files` into a fresh temp dir and returns it. The dir must stay
/// alive for the duration of the test - dropping it deletes the scripts.
fn workspace(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    for (name, body) in files {
        fs::write(dir.path().join(name), body).expect("write script");
    }
    dir
}

fn nib() -> Command {
    Command::cargo_bin("nib").expect("built binary")
}

// --- running --------------------------------------------------------------

#[test]
fn runs_a_script_and_prints_to_stdout() {
    let dir = workspace(&[("s.nib", r#"println("hello", 1, [2, 3]);"#)]);
    nib()
        .arg(dir.path().join("s.nib"))
        .assert()
        .success()
        .stdout("hello 1 [2, 3]\n");
}

/// `print` writes without a trailing newline; `println` adds one.
#[test]
fn print_and_println_differ_only_in_the_newline() {
    let dir = workspace(&[("s.nib", r#"print("a"); print("b"); println("c");"#)]);
    nib()
        .arg(dir.path().join("s.nib"))
        .assert()
        .success()
        .stdout("abc\n");
}

#[test]
fn read_consumes_a_line_of_stdin() {
    let dir = workspace(&[("s.nib", r#"var name = read(); println("hi " + name);"#)]);
    nib()
        .arg(dir.path().join("s.nib"))
        .write_stdin("world\n")
        .assert()
        .success()
        .stdout("hi world\n");
}

#[test]
fn read_rejects_arguments() {
    let dir = workspace(&[("s.nib", r#"var x = read(1);"#)]);
    nib()
        .arg(dir.path().join("s.nib"))
        .write_stdin("\n")
        .assert()
        .failure()
        .stderr(contains("'read' expects 0 arguments"));
}

#[test]
fn time_flag_reports_execution_time() {
    let dir = workspace(&[("s.nib", "var x = 1;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--time")
        .assert()
        .success()
        .stdout(contains("Execution time:"));
}

// --- includes -------------------------------------------------------------

#[test]
fn include_loads_a_library_before_the_script() {
    let dir = workspace(&[
        ("lib.nib", "func double(x) { return x * 2; }"),
        ("s.nib", "println(double(21));"),
    ]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--include")
        .arg(dir.path().join("lib.nib"))
        .assert()
        .success()
        .stdout("42\n");
}

#[test]
fn include_accepts_a_comma_separated_list_in_order() {
    let dir = workspace(&[
        ("a.nib", "func f() { return \"a\"; }"),
        ("b.nib", "func g() { return f() + \"b\"; }"),
        ("s.nib", "println(g());"),
    ]);
    let list = format!(
        "{},{}",
        dir.path().join("a.nib").display(),
        dir.path().join("b.nib").display()
    );
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--include")
        .arg(list)
        .assert()
        .success()
        .stdout("ab\n");
}

#[test]
fn errors_in_included_code_are_labelled() {
    let dir = workspace(&[("lib.nib", "var broken = 1 / 0;"), ("s.nib", "println(1);")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--include")
        .arg(dir.path().join("lib.nib"))
        .assert()
        .failure()
        .stderr(contains("in included code"));
}

// --- --ast ----------------------------------------------------------------

#[test]
fn ast_flag_prints_the_tree_instead_of_running() {
    // 42 only ever appears if the script *ran* - the tree holds 6 and 7
    let dir = workspace(&[("s.nib", "println(6 * 7);")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--ast")
        .assert()
        .success()
        .stdout(
            contains("Ast {")
                .and(contains("Binary"))
                .and(contains("42").not()),
        );
}

#[test]
fn ast_flag_with_a_path_writes_a_file() {
    let dir = workspace(&[("s.nib", "var x = 1 + 2;")]);
    let out = dir.path().join("tree.txt");
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--ast")
        .arg(&out)
        .assert()
        .success()
        .stdout(contains("AST written to"));
    assert!(fs::read_to_string(&out).unwrap().contains("Binary"));
}

#[test]
fn ast_flag_reports_an_unwritable_destination() {
    let dir = workspace(&[("s.nib", "var x = 1;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--ast")
        .arg(dir.path().join("no_such_dir").join("tree.txt"))
        .assert()
        .failure()
        .stderr(contains("Failed to write AST"));
}

// --- --check --------------------------------------------------------------

#[test]
fn check_reports_valid_syntax_without_running() {
    let dir = workspace(&[("s.nib", r#"println("should not run");"#)]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--check")
        .assert()
        .success()
        .stdout(contains("syntax OK").and(contains("should not run").not()));
}

#[test]
fn check_reports_a_syntax_error_in_the_script() {
    let dir = workspace(&[("s.nib", "var x = ;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--check")
        .assert()
        .failure()
        .stderr(contains("Parse error"));
}

/// `--check` parses each include independently, so a bad one is named.
#[test]
fn check_reports_a_syntax_error_in_an_include() {
    let dir = workspace(&[("lib.nib", "func broken( {"), ("s.nib", "var x = 1;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--check")
        .arg("--include")
        .arg(dir.path().join("lib.nib"))
        .assert()
        .failure()
        .stderr(contains("lib.nib").and(contains("error")));
}

/// A *runtime* error is not a syntax error - `--check` never runs the script.
#[test]
fn check_ignores_runtime_errors() {
    let dir = workspace(&[("s.nib", "var x = 1 / 0;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--check")
        .assert()
        .success()
        .stdout(contains("syntax OK"));
}

// --- failure modes --------------------------------------------------------

#[test]
fn a_missing_script_is_reported_and_fails() {
    nib()
        .arg("definitely_not_here.nib")
        .assert()
        .failure()
        .stderr(contains("failed to read script"));
}

#[test]
fn a_missing_include_is_reported_and_fails() {
    let dir = workspace(&[("s.nib", "var x = 1;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .arg("--include")
        .arg(dir.path().join("nope.nib"))
        .assert()
        .failure()
        .stderr(contains("failed to read include"));
}

#[test]
fn parse_errors_go_to_stderr_with_a_failing_exit_code() {
    let dir = workspace(&[("s.nib", "var x = ;")]);
    nib()
        .arg(dir.path().join("s.nib"))
        .assert()
        .failure()
        .stdout("")
        .stderr(contains("Parse error at 1:9"));
}

#[test]
fn runtime_errors_go_to_stderr_after_partial_output() {
    let dir = workspace(&[(
        "s.nib",
        r#"println("before"); var x = 1 / 0; println("after");"#,
    )]);
    nib()
        .arg(dir.path().join("s.nib"))
        .assert()
        .failure()
        .stdout("before\n")
        .stderr(contains("division by zero"));
}

#[test]
fn exit_stops_the_script_but_still_succeeds() {
    let dir = workspace(&[("s.nib", r#"println("a"); exit; println("b");"#)]);
    nib()
        .arg(dir.path().join("s.nib"))
        .assert()
        .success()
        .stdout("a\n");
}

// --- clap surface ---------------------------------------------------------

#[test]
fn version_and_help_are_available() {
    nib()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("nib"));
    nib()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Run a nib script").and(contains("--include")));
}

#[test]
fn a_missing_script_argument_is_a_usage_error() {
    nib().assert().failure().stderr(contains("Usage"));
}
