//! Typed member readers.
//!
//! Ports the `mandatory`/`optional` helpers from `src/cpp/generic_factory.h`.
//! Both honour the `was_loaded` flag: when an object has already been loaded
//! (typically a `copy-from` base), a missing member means "keep the inherited
//! value" instead of an error or a reset to the default.

use serde_json::Value;

use crate::error::JsonResult;
use crate::object::{json_type_name, JsonObject};

/// Types that can be read from a JSON value.
pub trait ReadJson: Sized {
    /// Read `value`, using `context` to build located errors.
    fn read_json(value: &Value, context: &JsonObject<'_>) -> JsonResult<Self>;
}

impl ReadJson for String {
    fn read_json(value: &Value, context: &JsonObject<'_>) -> JsonResult<Self> {
        match value {
            Value::String(text) => Ok(text.clone()),
            other => Err(context.throw_error(format!(
                "expected a string, but found {}",
                json_type_name(other)
            ))),
        }
    }
}

impl ReadJson for bool {
    fn read_json(value: &Value, context: &JsonObject<'_>) -> JsonResult<Self> {
        match value {
            Value::Bool(flag) => Ok(*flag),
            other => Err(context.throw_error(format!(
                "expected a boolean, but found {}",
                json_type_name(other)
            ))),
        }
    }
}

/// Implements `ReadJson` for integer types that must fit an exact range.
macro_rules! impl_read_integer {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ReadJson for $ty {
                fn read_json(value: &Value, context: &JsonObject<'_>) -> JsonResult<Self> {
                    value.as_i64().and_then(|number| <$ty>::try_from(number).ok()).ok_or_else(|| {
                        context.throw_error(format!(
                            "expected an integer that fits {}, but found {}",
                            stringify!($ty),
                            json_type_name(value)
                        ))
                    })
                }
            }
        )*
    };
}

impl_read_integer!(i32, i64, u32, u64, usize);

impl ReadJson for f64 {
    fn read_json(value: &Value, context: &JsonObject<'_>) -> JsonResult<Self> {
        value.as_f64().ok_or_else(|| {
            context.throw_error(format!(
                "expected a number, but found {}",
                json_type_name(value)
            ))
        })
    }
}

impl<T: ReadJson> ReadJson for Vec<T> {
    fn read_json(value: &Value, context: &JsonObject<'_>) -> JsonResult<Self> {
        match value {
            Value::Array(items) => items
                .iter()
                .map(|item| T::read_json(item, context))
                .collect(),
            other => Err(context.throw_error(format!(
                "expected an array, but found {}",
                json_type_name(other)
            ))),
        }
    }
}

/// Read a required member.
///
/// Returns `Ok(None)` when the member is absent **and** the object was already
/// loaded, which tells the caller to keep the inherited value. A missing member
/// on a freshly loaded object is an error.
pub fn mandatory<T: ReadJson>(
    jo: &JsonObject<'_>,
    was_loaded: bool,
    name: &str,
) -> JsonResult<Option<T>> {
    match jo.get_value(name) {
        Some(value) => T::read_json(value, jo).map(Some),
        None if was_loaded => Ok(None),
        None => Err(jo.throw_error(format!("missing mandatory member '{name}'"))),
    }
}

/// Read an optional member.
///
/// Returns `Ok(None)` when the member is absent. The `was_loaded` parameter is
/// accepted for symmetry with [`mandatory`]; unlike a mandatory member, an
/// absent optional member never errors, it just leaves the inherited value in
/// place.
pub fn optional<T: ReadJson>(
    jo: &JsonObject<'_>,
    _was_loaded: bool,
    name: &str,
) -> JsonResult<Option<T>> {
    match jo.get_value(name) {
        Some(value) => T::read_json(value, jo).map(Some),
        None => Ok(None),
    }
}

/// Read a string member that may also be given as a one-element array
/// (`"id": ["a"]`), matching the C++ `assign` behaviour for id members.
pub fn read_string_or_first_of_array(
    value: &Value,
    context: &JsonObject<'_>,
) -> JsonResult<String> {
    match value {
        Value::Array(items) => match items.first() {
            Some(first) => String::read_json(first, context),
            None => Err(context.throw_error("expected a non-empty array of strings")),
        },
        other => String::read_json(other, context),
    }
}
