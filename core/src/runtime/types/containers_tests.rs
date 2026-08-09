use super::*;

// `depth` is 0 for a leaf, so an array of scalars is 1; `nodes` counts the
// container itself plus every value in its subtree.
#[test]
fn new_caches_depth_and_nodes() {
    let flat = ArrayData::new(vec![Value::Int(1), Value::Int(2)]);
    assert_eq!((flat.depth, flat.nodes), (1, 3));

    let nested = ArrayData::new(vec![Value::Int(1), Value::array(vec![Value::Int(2)])]);
    assert_eq!((nested.depth, nested.nodes), (2, 4));

    let empty = ArrayData::new(vec![]);
    assert_eq!((empty.depth, empty.nodes), (1, 1));
}

#[test]
fn push_and_set_maintain_nodes_exactly() {
    let mut a = ArrayData::new(vec![Value::Int(1)]);
    assert_eq!(a.nodes, 2);

    a.push(Value::array(vec![Value::Int(2), Value::Int(3)]));
    assert_eq!(a.nodes, 5); // 2 + (1 container + 2 elements)
    assert_eq!(a.depth, 2);

    // set subtracts the old child and adds the new one
    a.set(1, Value::Int(9));
    assert_eq!(a.nodes, 3);
}

// `pop`/`remove_at` deliberately don't recompute depth downward - paying O(len)
// to shrink it isn't worth it, and a stale upper bound is always safe.
#[test]
fn pop_shrinks_nodes_exactly_but_leaves_depth_an_upper_bound() {
    let mut a = ArrayData::new(vec![Value::Int(1), Value::array(vec![Value::Int(2)])]);
    assert_eq!((a.depth, a.nodes), (2, 4));

    a.pop();
    assert_eq!(a.nodes, 2, "nodes is maintained exactly");
    assert_eq!(
        a.depth, 2,
        "depth is left as a safe upper bound, not recomputed"
    );
}

#[test]
fn node_arithmetic_saturates_rather_than_wrapping() {
    // `a = [a, a]` doubles nodes each round, reaching usize::MAX in 64 steps
    let mut a = ArrayData::new(vec![Value::Int(1)]);
    a.nodes = usize::MAX - 1;
    a.push(Value::Int(2));
    assert_eq!(a.nodes, usize::MAX);
}

#[test]
fn map_new_collapses_duplicate_keys_keeping_first_position() {
    let m = MapData::new(vec![
        ("a".into(), Value::Int(1)),
        ("b".into(), Value::Int(2)),
        ("a".into(), Value::Int(3)),
    ]);
    assert_eq!(m.len(), 2);
    assert_eq!(m.position("a"), Some(0), "first position is kept");
    assert_eq!(m.lookup("a"), Some(&Value::Int(3)), "last value wins");
}

// The side index and the Vec must stay in step: the Vec gives insertion order,
// the index stops every lookup and upsert being a linear scan.
#[test]
fn map_index_stays_in_step_with_the_vec() {
    let mut m = MapData::new(vec![("a".into(), Value::Int(1))]);
    m.insert("b".into(), Value::Int(2));
    m.insert("a".into(), Value::Int(9)); // upsert, not append

    assert_eq!(m.len(), 2);
    assert_eq!(m.position("a"), Some(0));
    assert_eq!(m.position("b"), Some(1));
    assert_eq!(m.lookup("a"), Some(&Value::Int(9)));
}

#[test]
fn remove_at_repairs_shifted_positions() {
    let mut m = MapData::new(vec![
        ("a".into(), Value::Int(1)),
        ("b".into(), Value::Int(2)),
        ("c".into(), Value::Int(3)),
    ]);
    let (key, value) = m.remove_at(0);
    assert_eq!((key.as_str(), value), ("a", Value::Int(1)));

    assert_eq!(m.position("a"), None);
    assert_eq!(
        m.position("b"),
        Some(0),
        "positions after the hole shift down"
    );
    assert_eq!(m.position("c"), Some(1));
    assert_eq!(m.lookup("c"), Some(&Value::Int(3)));
}

// Structural equality ignores the cached depth: it's an upper bound (see
// `pop`), so two equal arrays can legitimately carry different values for it.
#[test]
fn equality_ignores_the_cached_depth() {
    let mut popped = ArrayData::new(vec![Value::Int(1), Value::array(vec![Value::Int(2)])]);
    popped.pop();
    let fresh = ArrayData::new(vec![Value::Int(1)]);

    assert_ne!(popped.depth, fresh.depth, "the bound really is stale");
    assert_eq!(popped, fresh);
}

// The derived recursive drop walks one native stack frame per level and aborts
// the process on a deep enough value - and a Drop can't fail gracefully or be
// caught, so it's bounded structurally by an explicit worklist instead.
#[test]
fn deeply_nested_values_drop_without_overflowing_the_stack() {
    let mut deep = Value::Int(1);
    for _ in 0..200_000 {
        deep = Value::array(vec![deep]);
    }
    drop(deep); // the assertion is that this returns at all
}

// The map arm of the worklist teardown, which the array test above misses.
#[test]
fn deeply_nested_maps_drop_without_overflowing_the_stack() {
    let mut deep = Value::Int(1);
    for _ in 0..200_000 {
        deep = Value::map(vec![("k".into(), deep)]);
    }
    drop(deep);
}

// `Rc::into_inner` returns None when the payload is still shared, so the
// teardown must simply skip it and let the surviving owner free it later.
#[test]
fn shared_containers_are_left_for_their_other_owner() {
    let shared_map = Value::map(vec![("k".into(), Value::Int(1))]);
    let holder = Value::array(vec![shared_map.clone()]);
    drop(holder); // the inner map is still owned by `shared_map`

    let shared_arr = Value::array(vec![Value::Int(1)]);
    let holder2 = Value::array(vec![shared_arr.clone()]);
    drop(holder2);

    assert_eq!(shared_map, Value::map(vec![("k".into(), Value::Int(1))]));
    assert_eq!(shared_arr, Value::array(vec![Value::Int(1)]));
}
