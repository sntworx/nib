//! Calls, natives, and the no-closures scope contract -
//! `runtime/interpreter/calls.rs`.

use crate::common::{cfg, err, harness, run};
use nib_lang::Value;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn declaration_call_and_return() {
    let src = r#"func add(a, b) { return a + b; }
                 func noReturn() { let x = 1; }
                 out(add(2, 3), noReturn());"#;
    assert_eq!(run(src).unwrap(), ["5 null"]);
}

#[test]
fn arity_is_checked() {
    assert!(err("func f(a) { } f();").len() > 3);
    assert!(err("func f(a) { } f(1, 2);").len() > 3);
    assert_eq!(run("func f() { return 1; } out(f());").unwrap(), ["1"]);
}

#[test]
fn top_level_recursion_works() {
    let src = r#"func fact(n) { if n <= 1 { return 1; } return n * fact(n - 1); }
                 out(fact(10));"#;
    assert_eq!(run(src).unwrap(), ["3628800"]);
}

#[test]
fn mutual_recursion_works_at_top_level() {
    let src = r#"func isEven(n) { if n == 0 { return true; } return isOdd(n - 1); }
                 func isOdd(n) { if n == 0 { return false; } return isEven(n - 1); }
                 out(isEven(10), isOdd(7));"#;
    assert_eq!(run(src).unwrap(), ["true true"]);
}

/// A call strips every scope but the global one, so a function never sees the
/// caller's locals - in either direction. This is what makes cycles
/// unconstructible and reference counting sufficient.
#[test]
fn functions_do_not_capture_the_calling_scope() {
    assert!(
        err("func f() { return local; } func g() { let local = 1; return f(); } g();").len() > 3
    );
    assert!(err("func f() { let inner = 1; return 0; } f(); out(inner);").len() > 3);
}

#[test]
fn functions_see_globals_and_can_write_to_them() {
    let src = r#"let counter = 0;
                 func bump() { counter = counter + 1; }
                 bump(); bump();
                 out(counter);"#;
    assert_eq!(run(src).unwrap(), ["2"]);
}

/// Functions are ordinary values, which is what makes `array_map(a, dbl)`
/// work with no closures.
#[test]
fn functions_are_first_class_values() {
    let src = r#"func dbl(x) { return x * 2; }
                 func apply(fn, v) { return fn(v); }
                 let alias = dbl;
                 out(apply(dbl, 5), alias(3));
                 let fns = [dbl];
                 out(fns[0](7));
                 let m = {f: dbl};
                 out(m["f"](8));"#;
    assert_eq!(run(src).unwrap(), ["10 6", "14", "16"]);
}

#[test]
fn calling_a_non_function_is_an_error() {
    assert!(err("let x = 1; x();").contains("cannot call"));
    assert!(err("undefinedFn();").contains("undefined function"));
}

/// A call site's position survives the call, so an error *after* a successful
/// call reports the caller's line rather than a stale one from inside.
#[test]
fn error_positions_survive_a_call() {
    let e = err("func f() { return 1; }\nlet a = f();\nlet b = 1 / 0;");
    assert!(e.contains("3:"), "{}", e);
}

// --- native functions -----------------------------------------------------

#[test]
fn natives_validate_their_own_arguments() {
    let (mut nib, log) = harness(cfg(|_| {}));
    nib.register_func("half", |args: &[Value]| match args {
        [Value::Int(n)] => Ok(Value::Int(n / 2)),
        _ => Err(format!("half expects 1 int, got {} args", args.len())),
    });
    nib.parse(r#"out(half(10)); try { half(); } catch e { out(e); }"#)
        .unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["5", "half expects 1 int, got 0 args"]);
}

/// A native is just a `Value` in the global scope, so it survives the
/// scope-stripping a call does, and can be shadowed like any other global.
#[test]
fn natives_are_reachable_from_inside_functions_and_shadowable() {
    let (mut nib, log) = harness(cfg(|_| {}));
    nib.register_func("ext", |_: &[Value]| Ok(Value::Int(1)));
    nib.parse(
        r#"func usesExt() { return ext(); }
           out(usesExt());
           let ext = 5;
           out(ext);"#,
    )
    .unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["1", "5"]);
}

/// The closure is `Fn`, not `FnMut`, so mutable state needs interior
/// mutability - same as any shared Rust callback.
#[test]
fn natives_keep_state_via_interior_mutability() {
    let calls = Rc::new(RefCell::new(0));
    let counter = Rc::clone(&calls);
    let (mut nib, log) = harness(cfg(|_| {}));
    nib.register_func("tick", move |_: &[Value]| {
        *counter.borrow_mut() += 1;
        Ok(Value::Int(*counter.borrow()))
    });
    nib.parse("out(tick(), tick(), tick());").unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["1 2 3"]);
    assert_eq!(*calls.borrow(), 3);
}

#[test]
fn natives_receive_and_return_every_value_kind() {
    let (mut nib, log) = harness(cfg(|_| {}));
    nib.register_func("echo", |args: &[Value]| Ok(args[0].clone()));
    nib.register_func("kind", |args: &[Value]| {
        Ok(Value::Str(
            match &args[0] {
                Value::Int(_) => "int",
                Value::Float(_) => "float",
                Value::Str(_) => "str",
                Value::Bool(_) => "bool",
                Value::Array(_) => "array",
                Value::Map(_) => "map",
                Value::Null => "null",
                _ => "fn",
            }
            .to_string(),
        ))
    });
    nib.parse(
        r#"out(echo(1), echo(2.5), echo("s"), echo(true), echo(null));
           out(echo([1, [2]]), echo({a: {b: 1}}));
           func f() { } out(kind(f), kind([1]), kind({a: 1}), kind(null));"#,
    )
    .unwrap();
    nib.run().unwrap();
    assert_eq!(
        *log.borrow(),
        [
            "1 2.5 s true null",
            "[1, [2]] {a: {b: 1}}",
            "fn array map null"
        ]
    );
}

#[test]
fn natives_display_and_compare_by_identity() {
    let (mut nib, log) = harness(cfg(|_| {}));
    nib.register_func("ext", |_: &[Value]| Ok(Value::Null));
    nib.register_func("other", |_: &[Value]| Ok(Value::Null));
    nib.parse("let alias = ext; out(ext); out(ext == alias, ext == other);")
        .unwrap();
    nib.run().unwrap();
    assert_eq!(*log.borrow(), ["<native function ext>", "true false"]);
}

#[test]
fn natives_report_their_type_in_errors() {
    let (mut nib, _log) = harness(cfg(|_| {}));
    nib.register_func("ext", |_: &[Value]| Ok(Value::Null));
    nib.parse("out(ext[0]);").unwrap();
    let e = nib.run().unwrap_err().to_string();
    assert!(e.contains("cannot index into native function"), "{}", e);
}

/// `return` inside a loop propagates past the loop to the call boundary,
/// unlike `break`/`continue` which the loop intercepts.
#[test]
fn return_propagates_out_of_every_loop_form() {
    let src = r#"func w() { while true { return "w"; } }
                 func f() { for (;;) { return "f"; } }
                 func i() { for x in [1, 2] { return "i"; } }
                 func n() { for (let a = 0; a < 2; a++) { for x in [1] { return "n"; } } }
                 out(w(), f(), i(), n());"#;
    assert_eq!(run(src).unwrap(), ["w f i n"]);
}

/// `NativeFunction` has a hand-written `Debug` (its callback can't derive one).
#[test]
fn natives_are_debug_formattable() {
    let (mut nib, log) = harness(cfg(|_| {}));
    nib.register_func("ext", |_: &[Value]| Ok(Value::Null));
    nib.register_func("dbg", |args: &[Value]| {
        Ok(Value::Str(format!("{:?}", args[0])))
    });
    nib.parse("out(dbg(ext));").unwrap();
    nib.run().unwrap();
    assert!(
        log.borrow()[0].contains("NativeFunction(ext)"),
        "{:?}",
        log.borrow()
    );
}
