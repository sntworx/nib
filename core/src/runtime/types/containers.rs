// The payloads behind `Value::Array`/`Value::Map`. They live in their own
// module so the compiler enforces what was previously honor-system: `items`
// and `pairs` are private here, so nothing outside this file can mutate a
// container except through the methods below - the only things that keep the
// cached `depth`/`nodes` honest.

use std::collections::HashMap;
use std::rc::Rc;

use super::Value;

// Payload behind Value::Array. Caches `depth` (0 for a leaf, so an array of
// scalars is 1) purely so a nesting limit can be enforced in O(1) when a value
// is built - computing it on demand would be O(nodes). Every recursive walk
// over a Value burns one native stack frame per level, and `Drop` is the walk
// that can't fail gracefully, so the limit is what keeps them all safe.
// `Deref` gives read-only Vec access at the ~40 sites that only read; writes go
// through the methods below, which are the only things allowed to touch
// `items`, since they're what keep `depth` honest.
#[derive(Debug, Clone)]
pub struct ArrayData {
    items: Vec<Value>,
    depth: usize,
    nodes: usize,
}

impl ArrayData {
    pub(super) fn new(items: Vec<Value>) -> Self {
        let depth = items.iter().map(|v| v.depth() + 1).max().unwrap_or(1);
        let nodes = items
            .iter()
            .fold(1usize, |acc, v| acc.saturating_add(v.nodes()));
        ArrayData {
            items,
            depth,
            nodes,
        }
    }

    pub(super) fn push(&mut self, value: Value) {
        self.depth = self.depth.max(value.depth() + 1);
        self.nodes = self.nodes.saturating_add(value.nodes());
        self.items.push(value);
    }

    pub(super) fn pop(&mut self) -> Option<Value> {
        // `depth` deliberately isn't recomputed: shrinking can only lower it,
        // so the stale value stays a safe upper bound, and recomputing would
        // make `pop` O(len). `nodes` *is* exact - subtracting is O(1).
        let popped = self.items.pop();
        if let Some(value) = &popped {
            self.nodes = self.nodes.saturating_sub(value.nodes());
        }
        popped
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes
    }

    pub(crate) fn depth_count(&self) -> usize {
        self.depth
    }

    pub(crate) fn set(&mut self, index: usize, value: Value) {
        self.depth = self.depth.max(value.depth() + 1);
        self.nodes = self
            .nodes
            .saturating_sub(self.items[index].nodes())
            .saturating_add(value.nodes());
        self.items[index] = value;
    }
}

impl std::ops::Deref for ArrayData {
    type Target = Vec<Value>;
    fn deref(&self) -> &Vec<Value> {
        &self.items
    }
}

// Structural, ignoring the cached depth - it's an upper bound (see `pop`), so
// two equal arrays can legitimately carry different values for it.
impl PartialEq for ArrayData {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl Drop for ArrayData {
    fn drop(&mut self) {
        drop_nested(std::mem::take(&mut self.items));
    }
}

// Payload behind Value::Map - same rationale as ArrayData above, plus an
// `index` mapping key -> position in `pairs`. The Vec is what makes iteration
// order insertion order (the whole reason a map isn't a HashMap here); without
// the side index every lookup and every upsert was a linear scan, which made
// filling a map quadratic - 20k entries took 11s. The two must stay in step:
// only `insert`/`remove_at` may touch either.
#[derive(Debug, Clone)]
pub struct MapData {
    pairs: Vec<(String, Value)>,
    index: HashMap<String, usize>,
    depth: usize,
    nodes: usize,
}

impl MapData {
    pub(super) fn new(pairs: Vec<(String, Value)>) -> Self {
        let mut data = MapData {
            pairs: Vec::with_capacity(pairs.len()),
            index: HashMap::with_capacity(pairs.len()),
            depth: 1,
            nodes: 1,
        };
        // Sequential inserts also collapse duplicate literal keys (last wins,
        // first position kept), so callers don't dedupe separately.
        for (key, value) in pairs {
            data.insert(key, value);
        }
        data
    }

    pub(crate) fn position(&self, key: &str) -> Option<usize> {
        self.index.get(key).copied()
    }

    // Named `lookup`, not `get`, so it can't be confused with the `Vec::get`
    // reachable through Deref.
    pub(crate) fn lookup(&self, key: &str) -> Option<&Value> {
        self.position(key).map(|i| &self.pairs[i].1)
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes
    }

    pub(crate) fn depth_count(&self) -> usize {
        self.depth
    }

    // Upsert: the one write maps need, and the only place a map grows.
    pub(crate) fn insert(&mut self, key: String, value: Value) {
        self.depth = self.depth.max(value.depth() + 1);
        self.nodes = self.nodes.saturating_add(value.nodes());
        match self.position(&key) {
            Some(i) => {
                self.nodes = self.nodes.saturating_sub(self.pairs[i].1.nodes());
                self.pairs[i].1 = value;
            }
            None => {
                self.index.insert(key.clone(), self.pairs.len());
                self.pairs.push((key, value));
            }
        }
    }

    pub(super) fn remove_at(&mut self, index: usize) -> (String, Value) {
        // stale `depth` stays a safe upper bound, same as ArrayData::pop
        let removed = self.pairs.remove(index);
        self.index.remove(&removed.0);
        // Repairing the shifted positions is O(n), but `Vec::remove` already
        // shifted the same elements - no asymptotic change.
        for pos in self.index.values_mut() {
            if *pos > index {
                *pos -= 1;
            }
        }
        self.nodes = self.nodes.saturating_sub(removed.1.nodes());
        removed
    }
}

impl std::ops::Deref for MapData {
    type Target = Vec<(String, Value)>;
    fn deref(&self) -> &Vec<(String, Value)> {
        &self.pairs
    }
}

impl Drop for MapData {
    fn drop(&mut self) {
        drop_nested(self.pairs.drain(..).map(|(_, v)| v).collect());
    }
}

// Tears nested containers down with an explicit worklist. The derived
// recursive drop walks one native stack frame per nesting level and aborts the
// process on a deep enough value - and unlike a RuntimeError, a Drop can't
// fail gracefully or be caught, so it has to be bounded structurally rather
// than checked. Taking the children out of each node before it falls out of
// scope is what stops the recursion re-entering.
fn drop_nested(mut worklist: Vec<Value>) {
    while let Some(value) = worklist.pop() {
        match value {
            Value::Array(rc) => {
                if let Some(mut data) = Rc::into_inner(rc) {
                    worklist.append(&mut data.items);
                }
            }
            Value::Map(rc) => {
                if let Some(mut data) = Rc::into_inner(rc) {
                    worklist.extend(data.pairs.drain(..).map(|(_, v)| v));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "containers_tests.rs"]
mod tests;
