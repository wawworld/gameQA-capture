//! YAML three-tier merge logic.
//!
//! Merge semantics (FR-037, FR-038, FR-039):
//! - **Dict keys**: deep-merged (overlay wins for same key; base keys not overridden are kept).
//! - **Lists**: fully replaced (overlay's list replaces base's list entirely).
//! - **Null removes**: if the overlay value is `Null`, the key is removed from the result.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use serde_yaml::Value;

/// Merge `overlay` onto `base` using three-tier YAML merge semantics.
///
/// - Scalar overlay → replaces base scalar.
/// - Mapping overlay → recursively merged into base mapping.
/// - Sequence overlay → replaces base sequence entirely.
/// - Null overlay → removes the key from the result.
pub fn merge_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        // Both are mappings: deep-merge.
        (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
            for (k, v) in overlay_map {
                if v.is_null() {
                    // Null removes the key.
                    base_map.remove(&k);
                } else if let Some(base_val) = base_map.get(&k).cloned() {
                    // Recursive merge for nested mappings; replace for everything else.
                    base_map.insert(k, merge_values(base_val, v));
                } else {
                    base_map.insert(k, v);
                }
            }
            Value::Mapping(base_map)
        }
        // Overlay is not Null and not a mapping → replace base entirely.
        (_base, overlay) => overlay,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    fn yaml(s: &str) -> Value {
        serde_yaml::from_str(s).unwrap()
    }

    #[test]
    fn dict_deep_merge() {
        let base = yaml("a: 1\nb: 2");
        let overlay = yaml("b: 99\nc: 3");
        let result = merge_values(base, overlay);
        assert_eq!(result["a"], Value::Number(1.into()));
        assert_eq!(result["b"], Value::Number(99.into()));
        assert_eq!(result["c"], Value::Number(3.into()));
    }

    #[test]
    fn list_full_replace() {
        let base = yaml("items:\n  - a\n  - b");
        let overlay = yaml("items:\n  - c");
        let result = merge_values(base, overlay);
        let items = result["items"].as_sequence().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], Value::String("c".into()));
    }

    #[test]
    fn null_removes_key() {
        let base = yaml("a: 1\nb: 2");
        let overlay = yaml("b: ~");
        let result = merge_values(base, overlay);
        assert_eq!(result["a"], Value::Number(1.into()));
        assert!(result.get("b").is_none());
    }

    #[test]
    fn three_tier_composition() {
        let base = yaml("capture:\n  fps: 30\n  debug: false");
        let game = yaml("capture:\n  debug: true");
        let profile = yaml("capture:\n  fps: 60");

        let after_game = merge_values(base, game);
        let final_config = merge_values(after_game, profile);

        assert_eq!(final_config["capture"]["fps"], Value::Number(60.into()));
        assert_eq!(final_config["capture"]["debug"], Value::Bool(true));
    }

    #[test]
    fn scalar_overlay_replaces() {
        let base = yaml("x: hello");
        let overlay = yaml("x: world");
        let result = merge_values(base, overlay);
        assert_eq!(result["x"], Value::String("world".into()));
    }
}
