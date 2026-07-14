//! Payload shape classification (`ShapeClass`): `K = 64` sampling,
//! per-element key-signature, `uniformity_pct` computation.
//!
//! Descriptive/diagnostic only -- per `data-model.md`, `ShapeClass` does
//! NOT gate the conversion decision on its own; the byte-size comparison in
//! `convert.rs` is the sole gate.

use std::collections::HashMap;

use serde_json::Value;

/// Sampling cap for shape classification (data-model.md, plan.md).
pub const SHAPE_SAMPLE_CAP: usize = 64;

/// Uniformity fraction required to classify a sampled array as
/// `UniformArrayOfObjects` (data-model.md).
const UNIFORMITY_THRESHOLD: f64 = 0.9;

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeClass {
    UniformArrayOfObjects { uniformity_pct: f64, sampled: usize },
    Irregular,
    Scalar,
}

/// Classifies the top-level shape of `value`. A non-array root is always
/// `Scalar`; an array is sampled (up to `SHAPE_SAMPLE_CAP` elements) and
/// classified by how uniform its elements' key-signatures are.
pub fn classify(value: &Value) -> ShapeClass {
    match value {
        Value::Array(arr) => classify_array(arr),
        _ => ShapeClass::Scalar,
    }
}

/// Sorted key set for an object; the empty vector for any non-object
/// element (including an empty object) -- non-object elements are simply
/// elements that all share the same "no keys" signature as each other.
fn key_signature(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            keys
        }
        _ => Vec::new(),
    }
}

fn classify_array(arr: &[Value]) -> ShapeClass {
    if arr.is_empty() {
        return ShapeClass::Irregular;
    }

    let sampled: Vec<&Value> = arr.iter().take(SHAPE_SAMPLE_CAP).collect();
    let sample_count = sampled.len();

    let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
    for item in &sampled {
        let sig = key_signature(item);
        counts.entry(sig).and_modify(|c| *c += 1).or_insert(1);
    }

    // A plain running-max loop rather than `counts.values().max()` plus a
    // fallback: `sampled` is non-empty (arr isn't empty), so `counts` always
    // has at least one entry, but this form needs no fallback value at all
    // (Option-free), rather than papering over an unreachable `None` case.
    let mut max_count: usize = 0;
    for count in counts.values() {
        if *count > max_count {
            max_count = *count;
        }
    }

    let uniformity_pct = (max_count as f64) / (sample_count as f64);

    if uniformity_pct >= UNIFORMITY_THRESHOLD {
        ShapeClass::UniformArrayOfObjects { uniformity_pct, sampled: sample_count }
    } else {
        ShapeClass::Irregular
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn uniform_array_of_objects_above_threshold() {
        let arr = json!([
            {"a": 1, "b": 2},
            {"a": 3, "b": 4},
            {"a": 5, "b": 6},
        ]);
        match classify(&arr) {
            ShapeClass::UniformArrayOfObjects { uniformity_pct, sampled } => {
                assert!((uniformity_pct - 1.0).abs() < f64::EPSILON);
                assert_eq!(sampled, 3);
            }
            other => panic!("expected UniformArrayOfObjects, got {other:?}"),
        }
    }

    #[test]
    fn below_threshold_is_irregular() {
        let arr = json!([
            {"a": 1, "b": 2},
            {"a": 3, "c": 4},
            {"x": 1},
            {"y": 2},
        ]);
        assert_eq!(classify(&arr), ShapeClass::Irregular);
    }

    #[test]
    fn empty_array_is_irregular() {
        assert_eq!(classify(&json!([])), ShapeClass::Irregular);
    }

    #[test]
    fn non_array_root_is_scalar() {
        assert_eq!(classify(&json!({"a": 1})), ShapeClass::Scalar);
        assert_eq!(classify(&json!(42)), ShapeClass::Scalar);
        assert_eq!(classify(&json!("hello")), ShapeClass::Scalar);
        assert_eq!(classify(&Value::Null), ShapeClass::Scalar);
    }

    #[test]
    fn sampling_caps_at_k_64_even_for_larger_arrays() {
        let mut items: Vec<Value> = (0..500).map(|i| json!({"a": i, "b": i * 2})).collect();
        // Make every element past the K=64 sampling window irregular so a
        // naive "sample everything" implementation would drop uniformity
        // well below 0.9, but capping the sample at K=64 (all uniform)
        // must still classify this as UniformArrayOfObjects.
        for (i, item) in items.iter_mut().enumerate().skip(SHAPE_SAMPLE_CAP) {
            *item = json!({"different_key": i});
        }
        let arr = Value::Array(items);
        match classify(&arr) {
            ShapeClass::UniformArrayOfObjects { uniformity_pct, sampled } => {
                assert_eq!(sampled, SHAPE_SAMPLE_CAP);
                assert!((uniformity_pct - 1.0).abs() < f64::EPSILON);
            }
            other => {
                panic!("expected UniformArrayOfObjects (sampling capped at 64), got {other:?}")
            }
        }
    }
}
