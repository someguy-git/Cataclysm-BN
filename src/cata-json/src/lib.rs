//! JSON loading primitives for the Rust port.
//!
//! Phase 1 needs a JSON layer that behaves like the C++ `json.h`: it reports
//! errors with the file and line the value came from, tracks which members a
//! loader actually read (so silently ignored mod data gets flagged), and keeps
//! the `was_loaded`-aware `mandatory`/`optional` semantics used by
//! `copy-from` inheritance.
//!
//! The value model itself is `serde_json::Value`; this crate adds the loading
//! behaviour on top instead of re-implementing a parser.
//!
//! ```
//! use cata_json::{JsonObject, mandatory};
//! use serde_json::json;
//!
//! let value = json!({ "id": "example", "volume": 3 });
//! let members = value.as_object().expect("object");
//! let jo = JsonObject::new(members);
//! let id: Option<String> = mandatory(&jo, false, "id")?;
//! assert_eq!(id.as_deref(), Some("example"));
//! # Ok::<(), cata_json::JsonError>(())
//! ```

mod canonical;
mod error;
mod object;
mod readers;

pub use canonical::{to_canonical_string, write_canonical};
pub use error::{JsonError, JsonResult, SourceLocation};
pub use object::{json_type_name, JsonObject};
pub use readers::{mandatory, optional, read_string_or_first_of_array, ReadJson};

/// Parse JSON text, attaching `file` to every error raised while parsing.
///
/// Parsing has no per-value location in the C++ reader either, so the reported
/// location is the start of the file; callers that care about the exact line
/// build the object with [`JsonObject::new_at`] from a located reader.
pub fn parse_json(
    text: &str,
    file: impl Into<std::path::PathBuf>,
) -> JsonResult<serde_json::Value> {
    serde_json::from_str(text)
        .map_err(|err| JsonError::at(SourceLocation::new(file, 1, 1), err.to_string()))
}
