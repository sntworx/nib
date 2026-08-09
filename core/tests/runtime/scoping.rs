//! Scope stack behavior - `runtime/environment.rs`.

use crate::common::{err, run};

#[test]
fn blocks_create_and_drop_scopes() {
    assert_eq!(
        run("var x = 1; { var x = 2; out(x); } out(x);").unwrap(),
        ["2", "1"]
    );
    assert!(err("{ var inner = 1; } out(inner);").len() > 3);
}

#[test]
fn assignment_reaches_an_outer_scope_while_var_shadows() {
    assert_eq!(run("var x = 1; { x = 2; } out(x);").unwrap(), ["2"]);
    assert_eq!(run("var x = 1; { var x = 2; } out(x);").unwrap(), ["1"]);
}

/// `for`'s init lives in one scope for the loop's whole lifetime - visible to
/// cond/post/body, but not leaking out.
#[test]
fn for_init_is_scoped_to_the_loop() {
    assert_eq!(
        run("for (var i = 0; i < 2; i++) { out(i); }").unwrap(),
        ["0", "1"]
    );
    assert!(err("for (var i = 0; i < 1; i++) { } out(i);").len() > 3);
}

/// The for-in binding is fresh per iteration and doesn't leak either.
#[test]
fn for_in_binding_is_scoped_to_the_loop() {
    assert!(err("for x in [1] { } out(x);").len() > 3);
    // shadowing an outer name leaves it untouched
    assert_eq!(
        run("var x = 9; for x in [1, 2] { } out(x);").unwrap(),
        ["9"]
    );
}

#[test]
fn catch_binding_is_scoped_to_the_catch_block() {
    assert!(err(r#"try { throw "x"; } catch e { } out(e);"#).len() > 3);
    assert_eq!(
        run(r#"var e = 1; try { throw "x"; } catch e { } out(e);"#).unwrap(),
        ["1"]
    );
}

#[test]
fn if_and_while_bodies_have_their_own_scope() {
    assert!(err("if true { var a = 1; } out(a);").len() > 3);
    assert!(err("while true { var b = 1; break; } out(b);").len() > 3);
}

#[test]
fn redeclaring_a_global_overwrites_it() {
    assert_eq!(run("var x = 1; var x = 2; out(x);").unwrap(), ["2"]);
}
