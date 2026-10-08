//! The data layer: a `generic_factory` equivalent plus the conformance dump.
//!
//! Phase 1 ports the JSON loaders behind a registry that behaves like
//! `generic_factory` in `src/cpp/generic_factory.h`. The registry is what the
//! conformance harness compares against the C++ implementation, domain by
//! domain, using the canonical JSON of the loaded objects.
//!
//! ```
//! use cata_data::{GenericFactory, LoadContext, Loadable, LoadOutcome};
//! use cata_json::{JsonObject, JsonResult};
//!
//! #[derive(Default, Clone)]
//! struct Example {
//!     name: String,
//! }
//!
//! impl Loadable for Example {
//!     fn load(&mut self, jo: &JsonObject<'_>, _ctx: &LoadContext<'_>) -> JsonResult<()> {
//!         if let Some(name) = jo.get_string_opt("name")? {
//!             self.name = name.to_string();
//!         }
//!         Ok(())
//!     }
//! }
//!
//! let mut factory: GenericFactory<Example> = GenericFactory::new("example", "id", "alias");
//! let value = serde_json::json!({ "id": "a", "name": "first" });
//! let jo = JsonObject::new(value.as_object().expect("object"));
//! assert_eq!(factory.load(&jo, "dda")?, LoadOutcome::Loaded);
//! assert!(factory.is_valid("a"));
//! # Ok::<(), cata_json::JsonError>(())
//! ```

pub mod conformance;
mod factory;
mod loadable;

pub use conformance::{canonical_dump, ConformanceDump, ParseError};
pub use factory::{
    ConformanceEntry, ConsistencyError, DeferredEntry, GenericFactory, LoadOutcome, Loaded,
    RegistryDump, INVALID_VERSION,
};
pub use loadable::{LoadContext, Loadable};
