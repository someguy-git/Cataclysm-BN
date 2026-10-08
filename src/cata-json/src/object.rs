//! A JSON object view with member tracking and located errors.
//!
//! Mirrors `JsonObject` in `src/cpp/json.h`: it borrows a serde JSON object and
//! records which members were read so the loader can warn about members that
//! were silently ignored (a common source of mod bugs). Every failure it raises
//! carries the object's source location.

use std::cell::RefCell;
use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::error::{JsonError, JsonResult, SourceLocation};

/// A borrowed JSON object together with its source location.
#[derive(Debug)]
pub struct JsonObject<'a> {
    members: &'a Map<String, Value>,
    location: Option<SourceLocation>,
    visited: RefCell<HashSet<&'a str>>,
}

impl<'a> JsonObject<'a> {
    /// Wrap an object without a source location.
    pub fn new(members: &'a Map<String, Value>) -> Self {
        JsonObject {
            members,
            location: None,
            visited: RefCell::new(HashSet::new()),
        }
    }

    /// Wrap an object and remember where it was read from.
    pub fn new_at(members: &'a Map<String, Value>, location: SourceLocation) -> Self {
        JsonObject {
            members,
            location: Some(location),
            visited: RefCell::new(HashSet::new()),
        }
    }

    /// The location this object was read from, when known.
    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    /// The underlying member map. Used when an object has to be stored for a
    /// deferred retry pass instead of being loaded immediately.
    pub fn members(&self) -> &'a Map<String, Value> {
        self.members
    }

    /// Whether the object has a member with the given name.
    pub fn has_member(&self, name: &str) -> bool {
        self.mark_visited(name);
        self.members.contains_key(name)
    }

    /// Whether the member exists and holds a string.
    pub fn has_string(&self, name: &str) -> bool {
        self.get_value(name).is_some_and(Value::is_string)
    }

    /// Whether the member exists and holds an array.
    pub fn has_array(&self, name: &str) -> bool {
        self.get_value(name).is_some_and(Value::is_array)
    }

    /// Whether the member exists and holds an object.
    pub fn has_object(&self, name: &str) -> bool {
        self.get_value(name).is_some_and(Value::is_object)
    }

    /// The raw value of a member, if present.
    pub fn get_value(&self, name: &str) -> Option<&'a Value> {
        self.mark_visited(name);
        self.members.get(name)
    }

    /// The string held by a member, or an error naming the member.
    pub fn get_string(&self, name: &str) -> JsonResult<&'a str> {
        match self.get_value(name) {
            Some(Value::String(text)) => Ok(text),
            Some(other) => Err(self.error_for(name, other, "a string")),
            None => Err(self.throw_error(format!("missing member '{name}'"))),
        }
    }

    /// An optional string member; `None` when the member is absent.
    pub fn get_string_opt(&self, name: &str) -> JsonResult<Option<&'a str>> {
        match self.get_value(name) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(other) => Err(self.error_for(name, other, "a string")),
        }
    }

    /// The array held by a member, or an error naming the member.
    pub fn get_array(&self, name: &str) -> JsonResult<&'a Vec<Value>> {
        match self.get_value(name) {
            Some(Value::Array(items)) => Ok(items),
            Some(other) => Err(self.error_for(name, other, "an array")),
            None => Err(self.throw_error(format!("missing member '{name}'"))),
        }
    }

    /// The sub-object held by a member, or an error naming the member.
    pub fn get_object(&self, name: &str) -> JsonResult<JsonObject<'a>> {
        match self.get_value(name) {
            Some(Value::Object(members)) => Ok(match &self.location {
                Some(location) => JsonObject::new_at(members, location.clone()),
                None => JsonObject::new(members),
            }),
            Some(other) => Err(self.error_for(name, other, "an object")),
            None => Err(self.throw_error(format!("missing member '{name}'"))),
        }
    }

    /// The names of members that were read.
    pub fn visited_members(&self) -> HashSet<&'a str> {
        self.visited.borrow().clone()
    }

    /// The names of members that were never read.
    pub fn unvisited_members(&self) -> Vec<&'a str> {
        let visited = self.visited.borrow();
        self.members
            .keys()
            .map(String::as_str)
            .filter(|name| !visited.contains(name))
            .collect()
    }

    /// Mark the whole object as consumed, as inheritance handling does for
    /// deferred objects (the members are read on the retry pass).
    pub fn allow_omitted_members(&self) {
        let mut visited = self.visited.borrow_mut();
        for name in self.members.keys() {
            visited.insert(name.as_str());
        }
    }

    /// Build an error that includes the member name, type and location.
    pub fn error_for(&self, name: &str, value: &Value, expected: &str) -> JsonError {
        let actual = json_type_name(value);
        self.throw_error(format!(
            "member '{name}' must be {expected}, but is {actual}"
        ))
    }

    /// Build an error located at this object.
    pub fn throw_error(&self, message: impl Into<String>) -> JsonError {
        let error = JsonError::new(message);
        match &self.location {
            Some(location) => error.with_location(location.clone()),
            None => error,
        }
    }

    fn mark_visited(&self, name: &str) {
        if let Some((key, _)) = self.members.get_key_value(name) {
            self.visited.borrow_mut().insert(key.as_str());
        }
    }
}

/// The C++-style name of a JSON value's type, used in error messages.
pub fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
