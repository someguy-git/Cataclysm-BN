//! The object contract the data registry loads.
//!
//! Ports the requirements `generic_factory` places on `T` in
//! `src/cpp/generic_factory.h`: a default constructor (so `copy-from` can
//! clone a base and then override members) and a `load` that reads everything
//! except the id, which the factory owns.
//!
//! The C++ header stores `was_loaded` as a member of `T`; here the factory
//! passes it in [`LoadContext`] instead, which keeps the payload free of
//! loader bookkeeping.

use cata_json::{JsonObject, JsonResult};

/// Per-load state the factory hands to [`Loadable::load`].
#[derive(Clone, Copy, Debug)]
pub struct LoadContext<'a> {
    /// Identifier of the mod or core data set the object came from.
    pub src: &'a str,
    /// Whether a base object was inherited, in which case a missing member
    /// means "keep the inherited value" rather than "reset to the default".
    pub was_loaded: bool,
}

/// A type that can be loaded from a JSON object.
///
/// Implementors must not read the `id`/`ident` member; the factory assigns it
/// before calling [`Loadable::load`].
pub trait Loadable: Default + Clone {
    /// Read this object's members from `jo`.
    fn load(&mut self, jo: &JsonObject<'_>, ctx: &LoadContext<'_>) -> JsonResult<()>;

    /// Report a consistency problem with the fully loaded object.
    ///
    /// Called by [`crate::GenericFactory::check`] after finalization.
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}
