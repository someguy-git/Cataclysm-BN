//! The `generic_factory` port: a JSON-driven registry of objects keyed by id.
//!
//! The semantics follow `src/cpp/generic_factory.h` closely, because Phase 1's
//! conformance harness diffs these registries against the C++ ones:
//!
//! * objects are stored in load order and looked up by string id;
//! * `copy-from` clones a base object (loaded object or abstract) and the
//!   `was_loaded` flag makes missing members keep the inherited value;
//! * a `copy-from` whose base is not loaded yet defers the object, and
//!   [`GenericFactory::finalize`] retries it until no progress is made;
//! * `abstract` definitions are not registered as objects and are dropped at
//!   finalization;
//! * a version counter invalidates cached lookups on every mutation.

use std::collections::HashMap;

use cata_json::{JsonError, JsonObject, JsonResult, ReadJson, SourceLocation};
use serde_json::Value;

use crate::loadable::{LoadContext, Loadable};

/// Sentinel version that never matches a live factory version.
pub const INVALID_VERSION: u64 = u64::MAX;

const COPY_FROM_MEMBER: &str = "copy-from";
const ABSTRACT_MEMBER: &str = "abstract";
/// Temporary legacy id member kept for compatibility, as in the C++ factory.
const LEGACY_ID_MEMBER: &str = "ident";

/// One loaded object: its id plus the id-independent payload.
#[derive(Clone, Debug)]
pub struct Loaded<T> {
    /// The id the object was registered under.
    pub id: String,
    /// Identifier of the data set the object came from, for diagnostics.
    pub src: String,
    /// Whether [`Loadable::load`] may miss members because a base was inherited.
    pub was_loaded: bool,
    /// The payload itself.
    pub value: T,
}

impl<T: Default> Default for Loaded<T> {
    fn default() -> Self {
        Loaded {
            id: String::new(),
            src: String::new(),
            was_loaded: false,
            value: T::default(),
        }
    }
}

/// An object that can render itself as canonical JSON for the conformance dump.
pub trait ConformanceEntry {
    /// Stable JSON text for this object.
    fn canonical_json(&self) -> String;
}

/// A registry that can contribute entries to a conformance dump.
pub trait RegistryDump {
    /// The `type_name` the C++ factory was constructed with.
    fn type_name(&self) -> &str;

    /// `(id, canonical json)` pairs, in load order.
    fn dump_entries(&self) -> Vec<(String, String)>;
}

/// Whether a `load` call registered the object or queued it for a retry pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    /// The object (or each id in an id array) was registered.
    Loaded,
    /// A `copy-from` base was missing; the object was deferred.
    Deferred,
}

/// An object waiting for its `copy-from` base to be loaded.
#[derive(Clone, Debug)]
pub struct DeferredEntry {
    /// Where the object was read from.
    pub location: Option<SourceLocation>,
    /// The data set the object came from.
    pub src: String,
    /// The parsed object, retried verbatim during finalization.
    pub json: Value,
}

/// A consistency problem reported for a registered object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsistencyError {
    /// Id of the offending object.
    pub id: String,
    /// Human readable description.
    pub message: String,
}

/// A JSON-driven registry keyed by string id.
pub struct GenericFactory<T> {
    type_name: String,
    id_member_name: String,
    alias_member_name: String,
    version: u64,
    finalized: bool,
    list: Vec<Loaded<T>>,
    map: HashMap<String, usize>,
    abstracts: HashMap<String, Loaded<T>>,
    deferred: Vec<DeferredEntry>,
}

impl<T: Loadable> GenericFactory<T> {
    /// Create a factory.
    ///
    /// `type_name` is used in diagnostics ("vehicle type"), `id_member_name`
    /// names the JSON id member (usually `id`), and `alias_member_name` names
    /// the optional alias member.
    pub fn new(
        type_name: impl Into<String>,
        id_member_name: impl Into<String>,
        alias_member_name: impl Into<String>,
    ) -> Self {
        GenericFactory {
            type_name: type_name.into(),
            id_member_name: id_member_name.into(),
            alias_member_name: alias_member_name.into(),
            version: 0,
            finalized: false,
            list: Vec::new(),
            map: HashMap::new(),
            abstracts: HashMap::new(),
            deferred: Vec::new(),
        }
    }

    /// The name used in diagnostics.
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Whether [`GenericFactory::finalize`] has completed.
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// The number of registered objects.
    pub fn size(&self) -> usize {
        self.list.len()
    }

    /// Whether no object has been registered.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The current version, bumped on every mutation.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Number of objects still waiting for a `copy-from` base.
    pub fn deferred_len(&self) -> usize {
        self.deferred.len()
    }

    /// All registered objects, in load order.
    pub fn all(&self) -> &[Loaded<T>] {
        &self.list
    }

    /// The object registered under `id`.
    pub fn find(&self, id: &str) -> Option<&Loaded<T>> {
        self.map.get(id).and_then(|index| self.list.get(*index))
    }

    /// Whether an object is registered under `id`.
    pub fn is_valid(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    /// The index and object registered under `id`.
    pub fn get(&self, id: &str) -> Option<(usize, &Loaded<T>)> {
        let index = *self.map.get(id)?;
        Some((index, self.list.get(index)?))
    }

    /// Add an object, replacing any existing object with the same id.
    ///
    /// Returns the index it now occupies.
    pub fn insert(&mut self, object: Loaded<T>) -> usize {
        self.inc_version();
        if let Some(index) = self.map.get(&object.id).copied() {
            if let Some(slot) = self.list.get_mut(index) {
                *slot = object;
            }
            return index;
        }
        let index = self.list.len();
        self.map.insert(object.id.clone(), index);
        self.list.push(object);
        index
    }

    /// Load one JSON object, registering its id (or every id in an id array).
    pub fn load(&mut self, jo: &JsonObject<'_>, src: &str) -> JsonResult<LoadOutcome> {
        let mut def = Loaded::<T>::default();
        if !self.handle_inheritance(&mut def, jo, src)? {
            return Ok(LoadOutcome::Deferred);
        }

        let id_member = self.id_member_name.clone();
        if jo.has_string(&id_member) {
            let id = jo.get_string(&id_member)?.to_string();
            self.register_single(def, id, jo, src)?;
        } else if jo.has_array(&id_member) {
            self.register_array(jo, &id_member, src)?;
            self.reject_alias_with_array_id(jo, &id_member)?;
        } else if jo.has_string(LEGACY_ID_MEMBER) {
            let id = jo.get_string(LEGACY_ID_MEMBER)?.to_string();
            self.register_single(def, id, jo, src)?;
        } else if jo.has_array(LEGACY_ID_MEMBER) {
            self.register_array(jo, LEGACY_ID_MEMBER, src)?;
            self.reject_alias_with_array_id(jo, LEGACY_ID_MEMBER)?;
        } else if !jo.has_string(ABSTRACT_MEMBER) {
            return Err(jo.throw_error(format!(
                "must specify either '{ABSTRACT_MEMBER}' or '{0}'/'{LEGACY_ID_MEMBER}'",
                self.id_member_name
            )));
        }
        Ok(LoadOutcome::Loaded)
    }

    /// Resolve deferred objects, then drop the abstract definitions.
    ///
    /// Returns how many deferred objects this call resolved. Fails if the
    /// deferred set stops shrinking (a `copy-from` chain that cannot be
    /// satisfied) or if a retry reports an error.
    pub fn finalize(&mut self) -> JsonResult<usize> {
        if self.finalized {
            // Matches the C++ warning; finalizing twice is a loader bug, not
            // a data error, so it stays non-fatal.
            eprintln!(
                "Attempted to finalize {} factory multiple times.",
                self.type_name
            );
        }
        let mut resolved = 0;
        while !self.deferred.is_empty() {
            let pending = std::mem::take(&mut self.deferred);
            let pending_len = pending.len();
            let mut unresolved = Vec::new();
            for entry in pending {
                let Some(members) = entry.json.as_object() else {
                    return Err(JsonError::new(format!(
                        "deferred {} object is not a JSON object",
                        self.type_name
                    )));
                };
                let jo = match &entry.location {
                    Some(location) => JsonObject::new_at(members, location.clone()),
                    None => JsonObject::new(members),
                };
                match self.load(&jo, &entry.src) {
                    Ok(LoadOutcome::Deferred) => unresolved.push(entry),
                    Ok(LoadOutcome::Loaded) => resolved += 1,
                    Err(error) => return Err(error),
                }
            }
            self.deferred = unresolved;
            if self.deferred.len() == pending_len {
                return Err(JsonError::new(format!(
                    "{} unresolved copy-from {} object(s) after finalization",
                    self.deferred.len(),
                    self.type_name
                )));
            }
        }
        self.abstracts.clear();
        self.inc_version();
        self.finalized = true;
        Ok(resolved)
    }

    /// Run every object's consistency check.
    pub fn check(&self) -> Vec<ConsistencyError> {
        self.list
            .iter()
            .filter_map(|loaded| {
                loaded.value.check().err().map(|message| ConsistencyError {
                    id: loaded.id.clone(),
                    message,
                })
            })
            .collect()
    }

    /// Drop every object, abstract and deferred entry.
    pub fn reset(&mut self) {
        self.finalized = false;
        self.inc_version();
        self.list.clear();
        self.map.clear();
        self.abstracts.clear();
        self.deferred.clear();
    }

    /// The set of deferred objects, for a loader that owns the retry loop.
    pub fn take_deferred(&mut self) -> Vec<DeferredEntry> {
        std::mem::take(&mut self.deferred)
    }

    fn register_single(
        &mut self,
        mut def: Loaded<T>,
        id: String,
        jo: &JsonObject<'_>,
        src: &str,
    ) -> JsonResult<()> {
        def.id = id;
        def.src = src.to_string();
        let ctx = LoadContext {
            src,
            was_loaded: def.was_loaded,
        };
        def.value.load(jo, &ctx)?;
        let index = self.insert(def);
        let Some(object_id) = self.list.get(index).map(|loaded| loaded.id.clone()) else {
            return Ok(());
        };
        self.register_aliases(jo, &object_id)
    }

    fn register_array(
        &mut self,
        jo: &JsonObject<'_>,
        id_member: &str,
        src: &str,
    ) -> JsonResult<()> {
        for element in jo.get_array(id_member)? {
            let mut def = Loaded::<T>::default();
            if !self.handle_inheritance(&mut def, jo, src)? {
                // A deferred object would be retried by finalize(), which
                // would then register every id in the array again. Break out
                // exactly like the C++ loader does.
                break;
            }
            let id = String::read_json(element, jo)?;
            def.id = id;
            def.src = src.to_string();
            let ctx = LoadContext {
                src,
                was_loaded: def.was_loaded,
            };
            def.value.load(jo, &ctx)?;
            self.insert(def);
        }
        Ok(())
    }

    fn reject_alias_with_array_id(&self, jo: &JsonObject<'_>, id_member: &str) -> JsonResult<()> {
        if jo.has_member(&self.alias_member_name) {
            return Err(jo.throw_error(format!(
                "can not specify '{}' when '{id_member}' is array",
                self.alias_member_name
            )));
        }
        Ok(())
    }

    fn register_aliases(&mut self, jo: &JsonObject<'_>, object_id: &str) -> JsonResult<()> {
        if !jo.has_member(&self.alias_member_name) {
            return Ok(());
        }
        let Some(index) = self.map.get(object_id).copied() else {
            return Ok(());
        };
        let aliases = match jo.get_value(&self.alias_member_name) {
            Some(Value::String(alias)) => vec![alias.clone()],
            Some(value) => Vec::<String>::read_json(value, jo)?,
            None => return Ok(()),
        };
        // The C++ loader writes the alias entries straight into the map
        // without bumping the version; keep that behaviour so lookups cached
        // during loading are invalidated by the next mutation, not here.
        for alias in aliases {
            self.map.insert(alias, index);
        }
        Ok(())
    }

    /// Apply `copy-from` and `abstract` handling, mirroring the C++ loader.
    ///
    /// Returns `false` when a `copy-from` base is not available yet, which
    /// means the object was queued for the retry pass and must not be loaded.
    fn handle_inheritance(
        &mut self,
        def: &mut Loaded<T>,
        jo: &JsonObject<'_>,
        src: &str,
    ) -> JsonResult<bool> {
        if let Some(source) = jo.get_string_opt(COPY_FROM_MEMBER)? {
            let base = self
                .map
                .get(source)
                .and_then(|index| self.list.get(*index))
                .or_else(|| self.abstracts.get(source))
                .cloned();
            match base {
                Some(base) => *def = base,
                None => {
                    def.was_loaded = false;
                    jo.allow_omitted_members();
                    self.deferred.push(DeferredEntry {
                        location: jo.location().cloned(),
                        src: src.to_string(),
                        json: Value::Object(jo.members().clone()),
                    });
                    return Ok(false);
                }
            }
            def.was_loaded = true;
        }

        if let Some(name) = jo.get_string_opt(ABSTRACT_MEMBER)? {
            if jo.has_string(&self.id_member_name) || jo.has_string(LEGACY_ID_MEMBER) {
                return Err(jo.throw_error(format!(
                    "cannot specify both '{ABSTRACT_MEMBER}' and '{0}'/'{LEGACY_ID_MEMBER}'",
                    self.id_member_name
                )));
            }
            let ctx = LoadContext {
                src,
                was_loaded: def.was_loaded,
            };
            def.value.load(jo, &ctx)?;
            self.abstracts.insert(name.to_string(), def.clone());
        }
        Ok(true)
    }

    fn inc_version(&mut self) {
        loop {
            self.version = self.version.wrapping_add(1);
            if self.version != INVALID_VERSION {
                return;
            }
        }
    }
}

impl<T: Loadable + ConformanceEntry> RegistryDump for GenericFactory<T> {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn dump_entries(&self) -> Vec<(String, String)> {
        self.list
            .iter()
            .map(|loaded| (loaded.id.clone(), loaded.value.canonical_json()))
            .collect()
    }
}
