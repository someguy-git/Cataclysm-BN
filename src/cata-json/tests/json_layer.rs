//! Tests for the JSON loading layer against real engine data.
//!
//! The C++ loader is the oracle, but these tests also pin the behaviour the
//! conformance harness relies on: located errors, member tracking, and the
//! `was_loaded`-aware readers. The real-content test parses and re-parses an
//! actual `data/json` file, so a regression in canonicalisation shows up
//! immediately.

use std::path::PathBuf;

use cata_json::{
    mandatory, parse_json, to_canonical_string, JsonError, JsonObject, SourceLocation,
};
use serde_json::json;

fn repo_path(relative: &str) -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `src/cata-json`; the repository root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn mandatory_errors_when_missing_and_not_loaded() {
    let value = json!({});
    let jo = JsonObject::new(value.as_object().unwrap());
    let error: JsonError = mandatory::<String>(&jo, false, "id").unwrap_err();
    assert!(error.message().contains("missing mandatory member 'id'"));
}

#[test]
fn mandatory_keeps_inherited_value_when_loaded() {
    let value = json!({});
    let jo = JsonObject::new(value.as_object().unwrap());
    let read: Option<String> = mandatory(&jo, true, "id").unwrap();
    assert_eq!(read, None);
}

#[test]
fn mandatory_reports_the_wrong_type() {
    let value = json!({ "id": 7 });
    let jo = JsonObject::new(value.as_object().unwrap());
    let error = mandatory::<String>(&jo, false, "id").unwrap_err();
    assert!(error.message().contains("expected a string"));
}

#[test]
fn errors_carry_the_source_location() {
    let value = json!({ "volume": "big" });
    let location = SourceLocation::new("data/json/items/test.json", 12, 5);
    let jo = JsonObject::new_at(value.as_object().unwrap(), location.clone());
    let error: JsonError = mandatory::<f64>(&jo, false, "volume").unwrap_err();
    assert_eq!(error.location(), Some(&location));
    assert!(error
        .to_string()
        .starts_with("data/json/items/test.json:12:5:"));
}

#[test]
fn unvisited_members_are_reported() {
    let value = json!({ "id": "a", "typo_field": 1 });
    let jo = JsonObject::new(value.as_object().unwrap());
    let _: Option<String> = mandatory(&jo, false, "id").unwrap();
    assert_eq!(jo.unvisited_members(), vec!["typo_field"]);
}

#[test]
fn array_members_read_as_vectors_and_id_arrays() {
    let value = json!({ "id": ["a", "b"], "flags": ["X", "Y"] });
    let jo = JsonObject::new(value.as_object().unwrap());
    let flags: Option<Vec<String>> = mandatory(&jo, false, "flags").unwrap();
    assert_eq!(flags.unwrap(), vec!["X".to_string(), "Y".to_string()]);
}

#[test]
fn canonical_output_is_sorted_and_stable() {
    let first = json!({ "b": 1, "a": { "z": [1, 2], "y": "text\nwith\ttabs" } });
    let second = json!({ "a": { "y": "text\nwith\ttabs", "z": [1, 2] }, "b": 1 });
    assert_eq!(
        to_canonical_string(&first),
        to_canonical_string(&second),
        "member order must not affect the canonical form"
    );
    assert_eq!(
        to_canonical_string(&first),
        "{\"a\":{\"y\":\"text\\nwith\\ttabs\",\"z\":[1,2]},\"b\":1}"
    );
}

#[test]
fn parse_reports_the_file_on_failure() {
    let error = parse_json("{ nope }", "data/json/broken.json").unwrap_err();
    assert_eq!(
        error.location().map(|l| l.file().to_owned()),
        Some(PathBuf::from("data/json/broken.json"))
    );
}

#[test]
fn real_engine_data_round_trips_through_the_canonical_form() {
    let path = repo_path("data/json/materials.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    let parsed = parse_json(&text, &path).expect("materials.json must parse");
    let canonical = to_canonical_string(&parsed);

    // Canonical output must itself be valid JSON and canonicalise to itself.
    let reparsed = parse_json(&canonical, &path).expect("canonical output must parse");
    assert_eq!(canonical, to_canonical_string(&reparsed));

    let materials = parsed.as_array().expect("materials.json is an array");
    assert!(
        materials.len() > 50,
        "expected the full material list, found {}",
        materials.len()
    );
    // The one `copy-from` member must survive canonicalisation.
    assert!(canonical.contains("\"copy-from\""));
}
