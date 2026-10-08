//! The canonical dump the conformance harness diffs.
//!
//! Every ported domain contributes its types and objects to a
//! [`ConformanceDump`]. The dump is rendered as stable, sorted, tab-separated
//! text so it can be written to a golden file by the C++ implementation and by
//! the Rust port, then compared line by line.

use std::collections::BTreeMap;
use std::fmt;

use crate::factory::RegistryDump;

/// A canonical, comparable snapshot of every loaded object.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConformanceDump {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

/// A malformed line in a dump read back from a golden file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    line: usize,
    message: String,
}

impl ParseError {
    /// The 1-based line number the error is on.
    pub fn line(&self) -> usize {
        self.line
    }

    /// What was wrong with the line.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl ConformanceDump {
    /// An empty dump.
    pub fn new() -> Self {
        ConformanceDump::default()
    }

    /// Record one object.
    pub fn insert(
        &mut self,
        type_name: impl Into<String>,
        id: impl Into<String>,
        canonical_json: impl Into<String>,
    ) {
        self.sections
            .entry(type_name.into())
            .or_default()
            .insert(id.into(), canonical_json.into());
    }

    /// Record every entry of a registry.
    pub fn add_registry(&mut self, registry: &dyn RegistryDump) {
        for (id, canonical) in registry.dump_entries() {
            self.insert(registry.type_name(), id, canonical);
        }
    }

    /// Iterate over `(type_name, id, canonical json)` in stable order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.sections.iter().flat_map(|(type_name, ids)| {
            ids.iter()
                .map(move |(id, canonical)| (type_name.as_str(), id.as_str(), canonical.as_str()))
        })
    }

    /// The canonical JSON recorded for one object.
    pub fn get(&self, type_name: &str, id: &str) -> Option<&str> {
        self.sections
            .get(type_name)
            .and_then(|ids| ids.get(id))
            .map(String::as_str)
    }

    /// The number of recorded objects.
    pub fn len(&self) -> usize {
        self.sections.values().map(BTreeMap::len).sum()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Render the dump as sorted tab-separated text, one object per line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (type_name, ids) in &self.sections {
            for (id, canonical) in ids {
                out.push_str(type_name);
                out.push('\t');
                out.push_str(id);
                out.push('\t');
                out.push_str(canonical);
                out.push('\n');
            }
        }
        out
    }

    /// Read a dump back from its rendered form.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut dump = ConformanceDump::new();
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            if line.is_empty() {
                continue;
            }
            let mut fields = line.splitn(3, '\t');
            let (Some(type_name), Some(id), Some(canonical)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(ParseError {
                    line: line_number,
                    message: "expected type, id and canonical JSON separated by tabs".to_string(),
                });
            };
            dump.insert(type_name, id, canonical);
        }
        Ok(dump)
    }
}

/// Build a dump from a list of registries.
pub fn canonical_dump(registries: &[&dyn RegistryDump]) -> ConformanceDump {
    let mut dump = ConformanceDump::new();
    for registry in registries {
        dump.add_registry(*registry);
    }
    dump
}
