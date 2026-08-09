use super::*;

#[test]
fn define_and_get_across_scopes() {
    let mut env = Environment::new();
    env.define("g".to_string(), Value::Int(1));
    env.push_scope();
    env.define("l".to_string(), Value::Int(2));
    assert_eq!(env.get("g"), Some(&Value::Int(1)));
    assert_eq!(env.get("l"), Some(&Value::Int(2)));
    env.pop_scope();
    assert_eq!(env.get("l"), None, "locals die with their scope");
    assert_eq!(env.get("g"), Some(&Value::Int(1)));
}

#[test]
fn inner_scopes_shadow_without_clobbering() {
    let mut env = Environment::new();
    env.define("x".to_string(), Value::Int(1));
    env.push_scope();
    env.define("x".to_string(), Value::Int(2));
    assert_eq!(env.get("x"), Some(&Value::Int(2)));
    env.pop_scope();
    assert_eq!(env.get("x"), Some(&Value::Int(1)));
}

#[test]
fn assign_walks_outward_and_reports_unknown_names() {
    let mut env = Environment::new();
    env.define("x".to_string(), Value::Int(1));
    env.push_scope();
    assert!(env.assign("x", Value::Int(9)).is_ok());
    env.pop_scope();
    assert_eq!(env.get("x"), Some(&Value::Int(9)));
    assert!(env.assign("nope", Value::Int(1)).is_err());
}

// `take` leaves `Null` behind so the interpreter's copy is uniquely owned and
// `Rc::make_mut` can mutate in place instead of deep-copying.
#[test]
fn take_removes_the_value_and_leaves_null() {
    let mut env = Environment::new();
    env.define("a".to_string(), Value::array(vec![Value::Int(1)]));
    let taken = env.take("a");
    assert_eq!(taken, Some(Value::array(vec![Value::Int(1)])));
    assert_eq!(env.get("a"), Some(&Value::Null), "the slot is left as Null");
}

// The None arm is unreachable from script - a binding always exists by the
// time the interpreter detaches it - but it's the honest signature, so it's
// pinned here rather than left as the crate's only untested branch.
#[test]
fn take_returns_none_for_an_unbound_name() {
    let mut env = Environment::new();
    assert_eq!(env.take("never_defined"), None);
    env.push_scope();
    assert_eq!(env.take("still_not_defined"), None);
}

// A call strips every scope except the global one, then restores the caller's.
#[test]
fn enter_and_exit_call_strip_and_restore_scopes() {
    let mut env = Environment::new();
    env.define("global".to_string(), Value::Int(1));
    env.push_scope();
    env.define("caller_local".to_string(), Value::Int(2));

    let saved = env.enter_call();
    assert_eq!(env.get("global"), Some(&Value::Int(1)), "globals survive");
    assert_eq!(env.get("caller_local"), None, "caller locals do not");

    env.exit_call(saved);
    assert_eq!(env.get("caller_local"), Some(&Value::Int(2)), "restored");
}
