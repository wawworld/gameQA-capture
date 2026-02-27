//! T016 — YAML merge unit tests.
//!
//! Tests: dict deep-merge, list full-replace, null-removes-key, three-tier composition.

use game_qa::config::merge::merge_values;
use serde_yaml::Value;

fn yaml(s: &str) -> Value {
    serde_yaml::from_str(s).unwrap()
}

#[test]
fn dict_deep_merge_keeps_base_keys() {
    let base = yaml("a: 1\nb: 2\nc: 3");
    let overlay = yaml("b: 99");
    let result = merge_values(base, overlay);
    assert_eq!(result["a"], Value::Number(1.into()));
    assert_eq!(result["b"], Value::Number(99.into()));
    assert_eq!(result["c"], Value::Number(3.into()));
}

#[test]
fn nested_dict_deep_merge() {
    let base = yaml("x:\n  a: 1\n  b: 2");
    let overlay = yaml("x:\n  b: 99\n  c: 3");
    let result = merge_values(base, overlay);
    assert_eq!(result["x"]["a"], Value::Number(1.into()));
    assert_eq!(result["x"]["b"], Value::Number(99.into()));
    assert_eq!(result["x"]["c"], Value::Number(3.into()));
}

#[test]
fn list_full_replace() {
    let base = yaml("items:\n  - a\n  - b\n  - c");
    let overlay = yaml("items:\n  - x");
    let result = merge_values(base, overlay);
    let items = result["items"].as_sequence().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], Value::String("x".into()));
}

#[test]
fn null_removes_key() {
    let base = yaml("keep: 1\nremove: 2");
    let overlay = yaml("remove: ~");
    let result = merge_values(base, overlay);
    assert_eq!(result["keep"], Value::Number(1.into()));
    assert!(
        result.get("remove").is_none(),
        "null overlay must remove the key"
    );
}

#[test]
fn three_tier_composition() {
    // base ← game ← profile
    let base = yaml("capture:\n  fps: 30\n  debug: false\nroi:\n  threshold: 0.75");
    let game = yaml("roi:\n  threshold: 0.80\n  ref: dino.png");
    let profile = yaml("capture:\n  fps: 60");

    let after_game = merge_values(base, game);
    let final_config = merge_values(after_game, profile);

    assert_eq!(final_config["capture"]["fps"], Value::Number(60.into()));
    assert_eq!(final_config["capture"]["debug"], Value::Bool(false));
    assert_eq!(final_config["roi"]["threshold"].as_f64(), Some(0.80));
    assert_eq!(final_config["roi"]["ref"], Value::String("dino.png".into()));
}

#[test]
fn scalar_overlay_replaces_scalar() {
    let base = yaml("x: hello");
    let overlay = yaml("x: world");
    let result = merge_values(base, overlay);
    assert_eq!(result["x"], Value::String("world".into()));
}

#[test]
fn overlay_non_mapping_replaces_mapping() {
    // If overlay is a scalar over a mapping base, overlay wins entirely.
    let base = yaml("x:\n  a: 1");
    let overlay = yaml("x: flat");
    let result = merge_values(base, overlay);
    assert_eq!(result["x"], Value::String("flat".into()));
}
