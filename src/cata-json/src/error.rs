//! Error reporting with source locations.
//!
//! The C++ loader reports every JSON failure with the file and line the value
//! came from (`JsonIn::error`/`JsonObject::throw_error`), and the conformance
//! harness depends on that information to point at the offending file. This
//! module keeps the same information: a human readable message plus the
//! optional place it was raised.

use std::fmt;
use std::path::{Path, PathBuf};

/// Where a JSON value was read from: the file plus a 1-based line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    /// Path of the file the value came from.
    pub file: PathBuf,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
}

impl SourceLocation {
    /// Build a location from a file and a 1-based line/column pair.
    pub fn new(file: impl Into<PathBuf>, line: u32, column: u32) -> Self {
        SourceLocation {
            file: file.into(),
            line,
            column,
        }
    }

    /// The file this location points at.
    pub fn file(&self) -> &Path {
        &self.file
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file.display(), self.line, self.column)
    }
}

/// An error raised while loading JSON, optionally carrying its source location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    message: String,
    location: Option<SourceLocation>,
}

impl JsonError {
    /// Build an error with a message and no location.
    pub fn new(message: impl Into<String>) -> Self {
        JsonError {
            message: message.into(),
            location: None,
        }
    }

    /// Build an error tied to a specific source location.
    pub fn at(location: SourceLocation, message: impl Into<String>) -> Self {
        JsonError {
            message: message.into(),
            location: Some(location),
        }
    }

    /// Attach a source location if the error does not have one yet.
    #[must_use]
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        if self.location.is_none() {
            self.location = Some(location);
        }
        self
    }

    /// The error message, without the location prefix.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The location the error was raised at, when known.
    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(location) => write!(f, "{location}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for JsonError {}

/// Result alias for JSON loading.
pub type JsonResult<T> = Result<T, JsonError>;
