//! The JSON conformance harness.
//!
//! Phase 1's exit criterion is that the Rust data layer loads every
//! `data/json` file and every bundled mod to the same registry the C++ engine
//! produces. The mechanism is a golden dump: the C++ side renders its loaded
//! registries with [`cata_data::ConformanceDump`]'s format, the Rust side
//! renders the same format, and this crate reports the differing objects.
//!
//! ```
//! use cata_conformance::compare;
//!
//! let golden = "otype\tfoo\t{\"id\":\"foo\"}\n";
//! let rust = "otype\tfoo\t{\"id\":\"foo\"}\n";
//! assert!(compare(golden, rust)?.is_match());
//! # Ok::<(), cata_data::ParseError>(())
//! ```

mod diff;

pub use diff::{compare, diff, Difference, DifferenceKind, Report};

/// Compare a golden dump file with a Rust dump file, as `cata-conformance`
/// does from the command line.
pub fn compare_files(
    golden: impl AsRef<std::path::Path>,
    actual: impl AsRef<std::path::Path>,
) -> std::io::Result<Result<Report, cata_data::ParseError>> {
    let golden_text = std::fs::read_to_string(golden)?;
    let actual_text = std::fs::read_to_string(actual)?;
    Ok(compare(&golden_text, &actual_text))
}
