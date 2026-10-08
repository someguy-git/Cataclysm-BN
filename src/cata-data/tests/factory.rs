//! Behaviour tests for the `generic_factory` port.
//!
//! Each test mirrors a rule the C++ loader enforces in
//! `src/cpp/generic_factory.h`, so a divergence shows up as a failing test
//! rather than as a silent conformance difference later.

use cata_data::{ConformanceEntry, GenericFactory, LoadContext, LoadOutcome, Loadable, Loaded};
use cata_json::{mandatory, optional, JsonObject, JsonResult, SourceLocation};
use serde_json::{json, Value};

#[derive(Default, Clone, Debug, PartialEq)]
struct Demo {
    name: String,
    hp: i32,
    tags: Vec<String>,
}

impl Loadable for Demo {
    fn load(&mut self, jo: &JsonObject<'_>, ctx: &LoadContext<'_>) -> JsonResult<()> {
        if let Some(name) = mandatory::<String>(jo, ctx.was_loaded, "name")? {
            self.name = name;
        }
        if let Some(hp) = optional::<i32>(jo, ctx.was_loaded, "hp")? {
            self.hp = hp;
        }
        if let Some(tags) = optional::<Vec<String>>(jo, ctx.was_loaded, "tags")? {
            self.tags = tags;
        }
        Ok(())
    }

    fn check(&self) -> Result<(), String> {
        if self.name.is_empty() {
            Err("name is empty".to_string())
        } else {
            Ok(())
        }
    }
}

impl ConformanceEntry for Demo {
    fn canonical_json(&self) -> String {
        format!("{}|{}|{}", self.name, self.hp, self.tags.join(","))
    }
}

fn object(value: &Value) -> JsonObject<'_> {
    JsonObject::new(value.as_object().expect("test value must be an object"))
}

fn factory() -> GenericFactory<Demo> {
    GenericFactory::new("demo", "id", "alias")
}

#[test]
fn load_registers_a_single_id() {
    let mut factory = factory();
    let value = json!({ "id": "a", "name": "A", "hp": 3 });
    assert_eq!(
        factory.load(&object(&value), "core").expect("loads"),
        LoadOutcome::Loaded
    );
    assert!(factory.is_valid("a"));
    assert_eq!(factory.size(), 1);
    assert_eq!(
        factory.find("a").map(|l| l.value.clone()),
        Some(Demo {
            name: "A".to_string(),
            hp: 3,
            tags: Vec::new(),
        })
    );
}

#[test]
fn insert_replaces_an_existing_id_in_place() {
    let mut factory = factory();
    let first = Loaded {
        id: "a".to_string(),
        src: "core".to_string(),
        was_loaded: false,
        value: Demo {
            name: "first".to_string(),
            ..Demo::default()
        },
    };
    let second = Loaded {
        id: "a".to_string(),
        src: "mod".to_string(),
        was_loaded: false,
        value: Demo {
            name: "second".to_string(),
            ..Demo::default()
        },
    };
    let index = factory.insert(first);
    assert_eq!(factory.insert(second), index, "the slot is reused");
    assert_eq!(factory.size(), 1, "no duplicate entry is appended");
    assert_eq!(
        factory.find("a").map(|l| l.value.name.as_str()),
        Some("second")
    );
}

#[test]
fn copy_from_inherits_missing_members_and_overrides_present_ones() {
    let mut factory = factory();
    let base = json!({ "abstract": "base", "name": "Base", "hp": 5, "tags": ["x"] });
    factory.load(&object(&base), "core").expect("base loads");
    let child = json!({ "id": "child", "copy-from": "base", "hp": 9 });
    factory.load(&object(&child), "core").expect("child loads");
    let child = factory.find("child").expect("child registered");
    assert_eq!(child.value.name, "Base", "missing member is inherited");
    assert_eq!(child.value.hp, 9, "present member overrides the base");
    assert_eq!(child.value.tags, vec!["x".to_string()], "inherited array");
    assert!(child.was_loaded);
    assert!(!factory.is_valid("base"), "abstracts are not registered");
}

#[test]
fn copy_from_a_loaded_object_works_too() {
    let mut factory = factory();
    let base = json!({ "id": "base", "name": "Base", "hp": 4 });
    factory.load(&object(&base), "core").expect("base loads");
    let child = json!({ "id": "child", "copy-from": "base" });
    factory.load(&object(&child), "core").expect("child loads");
    assert_eq!(factory.find("child").map(|l| l.value.hp), Some(4));
}

#[test]
fn a_missing_base_defers_and_finalize_resolves_it() {
    let mut factory = factory();
    let child = json!({ "id": "child", "copy-from": "base", "hp": 9 });
    assert_eq!(
        factory.load(&object(&child), "core").expect("deferred"),
        LoadOutcome::Deferred
    );
    assert_eq!(factory.deferred_len(), 1);
    assert_eq!(factory.size(), 0, "a deferred object is not registered yet");

    let base = json!({ "abstract": "base", "name": "Base", "hp": 5, "tags": ["x"] });
    factory.load(&object(&base), "core").expect("base loads");
    assert_eq!(factory.finalize().expect("resolves"), 1);
    assert!(factory.is_valid("child"));
    assert_eq!(factory.find("child").map(|l| l.value.hp), Some(9));
    assert_eq!(
        factory.find("child").map(|l| l.value.name.as_str()),
        Some("Base")
    );
    assert_eq!(factory.deferred_len(), 0);
    assert!(factory.is_finalized());
}

#[test]
fn finalize_keeps_a_deferred_object_when_its_chain_is_not_ready() {
    let mut factory = factory();
    // `grandchild` copies `child`, which copies `base`; loading only `child`
    // leaves `grandchild` deferred across one finalize pass.
    let grandchild = json!({ "id": "grandchild", "copy-from": "child" });
    factory
        .load(&object(&grandchild), "core")
        .expect("deferred");
    let child = json!({ "id": "child", "copy-from": "base" });
    factory.load(&object(&child), "core").expect("deferred");

    // Nothing can be resolved yet, because `base` does not exist.
    let error = factory.finalize().expect_err("cannot resolve");
    assert!(
        error.message().contains("unresolved copy-from"),
        "unexpected error: {error}"
    );
}

#[test]
fn array_id_registers_every_entry() {
    let mut factory = factory();
    let base = json!({ "abstract": "base", "name": "Base", "hp": 5 });
    factory.load(&object(&base), "core").expect("base loads");
    let value = json!({ "id": ["a", "b", "c"], "copy-from": "base" });
    factory.load(&object(&value), "core").expect("array loads");
    assert_eq!(factory.size(), 3);
    for id in ["a", "b", "c"] {
        assert!(factory.is_valid(id), "{id} must be registered");
        assert_eq!(factory.find(id).map(|l| l.value.hp), Some(5));
    }
}

#[test]
fn legacy_ident_member_is_still_accepted() {
    let mut factory = factory();
    let value = json!({ "ident": "legacy", "name": "L" });
    factory.load(&object(&value), "core").expect("loads");
    assert!(factory.is_valid("legacy"));
}

#[test]
fn abstract_with_id_is_an_error() {
    let mut factory = factory();
    let value = json!({ "abstract": "a", "id": "b", "name": "n" });
    let error = factory.load(&object(&value), "core").expect_err("invalid");
    assert!(
        error.message().contains("cannot specify both"),
        "unexpected error: {error}"
    );
}

#[test]
fn alias_with_array_id_is_an_error() {
    let mut factory = factory();
    let value = json!({ "id": ["a"], "alias": ["x"], "name": "n" });
    let error = factory.load(&object(&value), "core").expect_err("invalid");
    assert!(
        error
            .message()
            .contains("can not specify 'alias' when 'id' is array"),
        "unexpected error: {error}"
    );
}

#[test]
fn an_object_without_id_or_abstract_is_an_error() {
    let mut factory = factory();
    let value = json!({ "name": "n" });
    let error = factory.load(&object(&value), "core").expect_err("invalid");
    assert!(
        error.message().contains("must specify either"),
        "unexpected error: {error}"
    );
}

#[test]
fn aliases_resolve_to_the_same_object() {
    let mut factory = factory();
    let value = json!({ "id": "a", "name": "A", "alias": ["b", "c"] });
    factory.load(&object(&value), "core").expect("loads");
    assert!(factory.is_valid("b"));
    assert!(factory.is_valid("c"));
    assert_eq!(
        factory.find("b").map(|l| l.id.as_str()),
        Some("a"),
        "an alias points at the canonical object"
    );
}

#[test]
fn a_single_string_alias_is_accepted() {
    let mut factory = factory();
    let value = json!({ "id": "a", "name": "A", "alias": "b" });
    factory.load(&object(&value), "core").expect("loads");
    assert!(factory.is_valid("b"));
}

#[test]
fn version_changes_on_every_mutation() {
    let mut factory = factory();
    let before = factory.version();
    let value = json!({ "id": "a", "name": "A" });
    factory.load(&object(&value), "core").expect("loads");
    assert_ne!(factory.version(), before);
    let after_load = factory.version();
    factory.insert(Loaded {
        id: "z".to_string(),
        src: "core".to_string(),
        was_loaded: false,
        value: Demo::default(),
    });
    assert_ne!(factory.version(), after_load);
}

#[test]
fn reset_clears_everything() {
    let mut factory = factory();
    let value = json!({ "id": "a", "name": "A" });
    factory.load(&object(&value), "core").expect("loads");
    assert!(factory.finalize().is_ok());
    factory.reset();
    assert_eq!(factory.size(), 0);
    assert_eq!(factory.deferred_len(), 0);
    assert!(!factory.is_finalized());
    assert!(!factory.is_valid("a"));
}

#[test]
fn check_reports_every_inconsistent_object() {
    let mut factory = factory();
    factory.insert(Loaded {
        id: "empty".to_string(),
        src: "core".to_string(),
        was_loaded: false,
        value: Demo::default(),
    });
    let errors = factory.check();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors.first().map(|e| e.id.as_str()), Some("empty"));
    assert_eq!(
        errors.first().map(|e| e.message.as_str()),
        Some("name is empty")
    );
}

#[test]
fn errors_carry_the_source_location_of_the_offending_object() {
    let mut factory = factory();
    let value = json!({ "id": "a" });
    let location = SourceLocation::new("data/json/demo.json", 42, 1);
    let jo = JsonObject::new_at(value.as_object().unwrap(), location.clone());
    let error = factory.load(&jo, "core").expect_err("missing name");
    assert_eq!(error.location(), Some(&location));
}

#[test]
fn objects_keep_their_load_order() {
    let mut factory = factory();
    for id in ["c", "a", "b"] {
        let value = json!({ "id": id, "name": id });
        factory.load(&object(&value), "core").expect("loads");
    }
    let ids: Vec<&str> = factory.all().iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, vec!["c", "a", "b"]);
}
