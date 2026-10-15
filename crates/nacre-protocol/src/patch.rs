//! RFC 7396 merge patch. Null members that cannot be introduced require a snapshot.
use serde_json::{Map, Value};

pub fn apply(target: &mut Value, patch: &Value) {
    if let Value::Object(changes) = patch {
        if !target.is_object() {
            *target = Value::Object(Map::new());
        }
        let target = target.as_object_mut().expect("object initialized");
        for (key, value) in changes {
            if value.is_null() {
                target.remove(key);
            } else {
                apply(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    } else {
        *target = patch.clone();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Diff {
    Unchanged,
    Patch(Value),
    Snapshot(Value),
}
fn candidate(before: &Value, after: &Value) -> Value {
    if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
        let mut changed = Map::new();
        for key in before.keys() {
            if !after.contains_key(key) {
                changed.insert(key.clone(), Value::Null);
            }
        }
        for (key, value) in after {
            if before.get(key) != Some(value) {
                changed.insert(
                    key.clone(),
                    candidate(before.get(key).unwrap_or(&Value::Null), value),
                );
            }
        }
        Value::Object(changed)
    } else {
        after.clone()
    }
}
pub fn diff(before: &Value, after: &Value) -> Diff {
    if before == after {
        return Diff::Unchanged;
    }
    let change = candidate(before, after);
    let mut result = before.clone();
    apply(&mut result, &change);
    if result == *after {
        Diff::Patch(change)
    } else {
        Diff::Snapshot(after.clone())
    }
}
pub fn apply_diff(before: &mut Value, change: &Diff) {
    match change {
        Diff::Unchanged => {}
        Diff::Patch(p) => apply(before, p),
        Diff::Snapshot(s) => *before = s.clone(),
    }
}
