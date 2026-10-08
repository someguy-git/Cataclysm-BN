//! Harness tests, including a load of a real `data/json` file.
//!
//! The last tests are the important ones: they run the ported registry over an
//! actual engine data file that uses an `abstract` definition, a single-id
//! `copy-from` and an id-array `copy-from`, and check that loading the file in
//! reverse order (which forces the deferred path) produces the identical
//! canonical dump. That is the mechanism the Phase 1 conformance oracle uses.

use std::path::PathBuf;

use cata_conformance::{compare, diff, DifferenceKind};
use cata_data::{
    canonical_dump, ConformanceDump, ConformanceEntry, GenericFactory, LoadContext, LoadOutcome,
    Loadable, RegistryDump,
};
use cata_json::{to_canonical_string, JsonObject, JsonResult};
use serde_json::Value;

const FIXTURE: &str = "data/json/overmap/overmap_terrain/overmap_terrain_private_resort.json";

/// Minimal stand-in for `overmap_terrain` that exercises inheritance.
#[derive(Default, Clone, Debug, PartialEq)]
struct Terrain {
    name: String,
    sym: String,
    color: String,
    flags: Vec<String>,
}

impl Loadable for Terrain {
    fn load(&mut self, jo: &JsonObject<'_>, ctx: &LoadContext<'_>) -> JsonResult<()> {
        if let Some(name) = cata_json::mandatory::<String>(jo, ctx.was_loaded, "name")? {
            self.name = name;
        }
        if let Some(sym) = cata_json::mandatory::<String>(jo, ctx.was_loaded, "sym")? {
            self.sym = sym;
        }
        if let Some(color) = cata_json::mandatory::<String>(jo, ctx.was_loaded, "color")? {
            self.color = color;
        }
        if let Some(flags) = cata_json::optional::<Vec<String>>(jo, ctx.was_loaded, "flags")? {
            self.flags = flags;
        }
        Ok(())
    }
}

impl ConformanceEntry for Terrain {
    fn canonical_json(&self) -> String {
        to_canonical_string(&serde_json::json!({
            "name": self.name,
            "sym": self.sym,
            "color": self.color,
            "flags": self.flags,
        }))
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Forward,
    Reverse,
}

/// Load the fixture into a factory, optionally in reverse file order.
fn load_fixture(order: Order) -> (GenericFactory<Terrain>, Value) {
    let path = repo_root().join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    let parsed = cata_json::parse_json(&text, &path).expect("fixture parses");
    let entries = parsed.as_array().expect("fixture is an array").clone();

    let mut factory: GenericFactory<Terrain> =
        GenericFactory::new("overmap_terrain", "id", "alias");
    let objects: Vec<&Value> = match order {
        Order::Forward => entries.iter().collect(),
        Order::Reverse => entries.iter().rev().collect(),
    };
    for value in objects {
        let jo = JsonObject::new(value.as_object().expect("object"));
        // Both outcomes are valid here: a forward load registers immediately,
        // a reverse load defers the `copy-from` objects until finalization.
        let _ = factory
            .load(&jo, "dda")
            .unwrap_or_else(|err| panic!("load failed: {err}"));
    }
    (factory, parsed)
}

#[test]
fn real_fixture_loads_abstract_and_both_copy_from_forms() {
    let (factory, _) = load_fixture(Order::Forward);
    assert_eq!(factory.size(), 32, "1 single id + 31 array ids");
    assert!(
        !factory.is_valid("generic_private_resort"),
        "the abstract must not be registered as an object"
    );

    let child = factory.find("p_resort_1nw").expect("copy-from child");
    assert_eq!(child.value.sym, "P", "the child overrides sym");
    assert_eq!(child.value.name, "private resort", "name is inherited");
    assert_eq!(child.value.color, "yellow", "color is inherited");
    assert_eq!(
        child.value.flags,
        vec![
            "RISK_HIGH".to_string(),
            "SOURCE_DRINK".to_string(),
            "SOURCE_LUXURY".to_string()
        ]
    );

    let sibling = factory.find("p_resort_0nw").expect("array id child");
    assert_eq!(sibling.value.sym, "p", "the abstract's sym is inherited");
    assert_eq!(sibling.value.name, "private resort");
}

#[test]
fn reverse_order_defers_and_finalizes_to_the_same_dump() {
    let (mut forward, _) = load_fixture(Order::Forward);
    forward
        .finalize()
        .expect("a forward load has nothing to resolve");
    let mut reverse = load_fixture(Order::Reverse).0;
    assert!(
        reverse.deferred_len() > 0,
        "loading the children first must defer them"
    );
    assert!(
        reverse.size() < 32,
        "deferred objects are not registered yet"
    );
    let resolved = reverse.finalize().expect("all deferrals resolve");
    assert!(resolved > 0);
    assert_eq!(reverse.size(), 32);

    let forward_dump = canonical_dump(&[&forward as &dyn RegistryDump]);
    let reverse_dump = canonical_dump(&[&reverse as &dyn RegistryDump]);
    let report = diff(&forward_dump, &reverse_dump);
    assert!(
        report.is_match(),
        "deferred resolution changed the registry:\n{}",
        report.summary()
    );
}

#[test]
fn dump_render_and_parse_round_trip() {
    let mut dump = ConformanceDump::new();
    dump.insert("otype", "a", "{\"id\":\"a\"}");
    dump.insert("itype", "b", "{\"id\":\"b\"}");
    let text = dump.render();
    assert_eq!(
        text, "itype\tb\t{\"id\":\"b\"}\notype\ta\t{\"id\":\"a\"}\n",
        "sections and ids must be sorted for a stable dump"
    );
    assert_eq!(ConformanceDump::parse(&text).expect("parses"), dump);
}

#[test]
fn diff_reports_missing_extra_and_changed_objects() {
    let mut expected = ConformanceDump::new();
    expected.insert("otype", "same", "{\"a\":1}");
    expected.insert("otype", "gone", "{\"a\":2}");
    expected.insert("otype", "edited", "{\"a\":3}");

    let mut actual = ConformanceDump::new();
    actual.insert("otype", "same", "{\"a\":1}");
    actual.insert("otype", "edited", "{\"a\":4}");
    actual.insert("otype", "new", "{\"a\":5}");

    let report = diff(&expected, &actual);
    assert!(!report.is_match());
    assert_eq!(report.differences().len(), 3);
    let kinds: Vec<DifferenceKind> = report
        .differences()
        .iter()
        .map(|difference| difference.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            DifferenceKind::Missing,
            DifferenceKind::Extra,
            DifferenceKind::Changed
        ],
        "differences are grouped by kind so reports are deterministic"
    );
    assert!(report.summary().contains("gone"));
    assert!(report.summary().contains("expected: {\"a\":3}"));
    assert!(report.summary().contains("actual:   {\"a\":4}"));
}

#[test]
fn compare_accepts_matching_text_and_rejects_malformed_lines() {
    let golden = "otype\ta\t{\"id\":\"a\"}\n";
    assert!(compare(golden, golden).expect("parses").is_match());

    let error = compare(golden, "otype-only\n").expect_err("malformed");
    assert_eq!(error.line(), 1);
    assert!(error
        .message()
        .contains("expected type, id and canonical JSON"));
}

#[test]
fn a_reloaded_registry_matches_a_fresh_one() {
    // The harness relies on the registry being deterministic: loading the same
    // file twice must produce byte-identical dumps.
    let first = load_fixture(Order::Forward).0;
    let second = load_fixture(Order::Forward).0;
    let report = diff(
        &canonical_dump(&[&first as &dyn RegistryDump]),
        &canonical_dump(&[&second as &dyn RegistryDump]),
    );
    assert!(report.is_match(), "{}", report.summary());
}

#[test]
fn loading_outcome_depends_on_whether_the_base_is_known() {
    let path = repo_root().join(FIXTURE);
    let text = std::fs::read_to_string(&path).expect("fixture readable");
    let parsed = cata_json::parse_json(&text, &path).expect("parses");
    let entries = parsed.as_array().expect("array").clone();
    let abstract_jo = JsonObject::new(entries[0].as_object().expect("object"));
    let child_jo = JsonObject::new(entries[1].as_object().expect("object"));

    // Loading the abstract first makes the child resolvable immediately.
    let mut in_order: GenericFactory<Terrain> =
        GenericFactory::new("overmap_terrain", "id", "alias");
    assert_eq!(
        in_order.load(&abstract_jo, "dda").expect("abstract loads"),
        LoadOutcome::Loaded
    );
    assert_eq!(
        in_order.load(&child_jo, "dda").expect("child loads"),
        LoadOutcome::Loaded
    );

    // Loading the child first defers it until the abstract exists.
    let mut out_of_order: GenericFactory<Terrain> =
        GenericFactory::new("overmap_terrain", "id", "alias");
    assert_eq!(
        out_of_order.load(&child_jo, "dda").expect("child defers"),
        LoadOutcome::Deferred
    );
    assert_eq!(out_of_order.deferred_len(), 1);
}
