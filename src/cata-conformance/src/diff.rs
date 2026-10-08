//! Diffs between a golden (C++) dump and a Rust dump.

use std::collections::BTreeSet;

use cata_data::{ConformanceDump, ParseError};

/// How a single object differs between the two dumps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DifferenceKind {
    /// The C++ dump has the object and the Rust dump does not.
    Missing,
    /// The Rust dump has the object and the C++ dump does not.
    Extra,
    /// Both have the object but its canonical JSON differs.
    Changed,
}

impl DifferenceKind {
    /// A short label for reports.
    pub fn as_str(&self) -> &'static str {
        match self {
            DifferenceKind::Missing => "missing",
            DifferenceKind::Extra => "extra",
            DifferenceKind::Changed => "changed",
        }
    }
}

/// One differing object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The type the object was registered under.
    pub type_name: String,
    /// The object's id.
    pub id: String,
    /// What kind of difference this is.
    pub kind: DifferenceKind,
    /// The canonical JSON from the golden dump, when present.
    pub expected: Option<String>,
    /// The canonical JSON from the Rust dump, when present.
    pub actual: Option<String>,
}

/// The result of comparing two dumps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    differences: Vec<Difference>,
    expected_len: usize,
    actual_len: usize,
}

impl Report {
    /// Whether the two dumps are identical.
    pub fn is_match(&self) -> bool {
        self.differences.is_empty()
    }

    /// Every differing object.
    pub fn differences(&self) -> &[Difference] {
        &self.differences
    }

    /// Number of objects in the golden dump.
    pub fn expected_len(&self) -> usize {
        self.expected_len
    }

    /// Number of objects in the Rust dump.
    pub fn actual_len(&self) -> usize {
        self.actual_len
    }

    /// A human readable summary, empty when the dumps match.
    pub fn summary(&self) -> String {
        if self.is_match() {
            return format!("match: {} object(s)", self.expected_len);
        }
        let mut out = String::new();
        out.push_str(&format!(
            "{} difference(s) between {} golden and {} rust object(s)\n",
            self.differences.len(),
            self.expected_len,
            self.actual_len
        ));
        for difference in &self.differences {
            out.push_str(&format!(
                "  {} {}: {}\n",
                difference.kind.as_str(),
                difference.type_name,
                difference.id
            ));
            if let Some(expected) = &difference.expected {
                out.push_str(&format!("    expected: {expected}\n"));
            }
            if let Some(actual) = &difference.actual {
                out.push_str(&format!("    actual:   {actual}\n"));
            }
        }
        out
    }
}

/// Compare a golden dump against a Rust dump.
pub fn diff(expected: &ConformanceDump, actual: &ConformanceDump) -> Report {
    let mut differences: Vec<Difference> = Vec::new();
    let expected_keys = keys(expected);
    let actual_keys = keys(actual);

    for (type_name, id) in expected_keys.difference(&actual_keys) {
        differences.push(Difference {
            type_name: type_name.clone(),
            id: id.clone(),
            kind: DifferenceKind::Missing,
            expected: expected.get(type_name, id).map(str::to_string),
            actual: None,
        });
    }
    for (type_name, id) in actual_keys.difference(&expected_keys) {
        differences.push(Difference {
            type_name: type_name.clone(),
            id: id.clone(),
            kind: DifferenceKind::Extra,
            expected: None,
            actual: actual.get(type_name, id).map(str::to_string),
        });
    }
    for (type_name, id) in expected_keys.intersection(&actual_keys) {
        let expected_json = expected.get(type_name, id).unwrap_or_default();
        let actual_json = actual.get(type_name, id).unwrap_or_default();
        if expected_json != actual_json {
            differences.push(Difference {
                type_name: type_name.clone(),
                id: id.clone(),
                kind: DifferenceKind::Changed,
                expected: Some(expected_json.to_string()),
                actual: Some(actual_json.to_string()),
            });
        }
    }

    Report {
        differences,
        expected_len: expected.len(),
        actual_len: actual.len(),
    }
}

/// Compare two dumps in their rendered form.
pub fn compare(expected_text: &str, actual_text: &str) -> Result<Report, ParseError> {
    let expected = ConformanceDump::parse(expected_text)?;
    let actual = ConformanceDump::parse(actual_text)?;
    Ok(diff(&expected, &actual))
}

fn keys(dump: &ConformanceDump) -> BTreeSet<(String, String)> {
    dump.iter()
        .map(|(type_name, id, _)| (type_name.to_string(), id.to_string()))
        .collect()
}
