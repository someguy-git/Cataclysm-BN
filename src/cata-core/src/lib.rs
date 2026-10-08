//! Core leaf types and helpers, ported incrementally from the C++ engine.
//!
//! This is the Phase 0 foundation crate. Modules are ported one at a time and
//! validated against the C++ implementation (the conformance oracle). Only the
//! modules listed below exist so far; the remaining leaf modules named in the
//! plan (`calendar`, `units`, `damage`, `coordinates`, `type_id`/`string_id`,
//! `cata_variant`) are added in follow-up work.

pub mod rng;
