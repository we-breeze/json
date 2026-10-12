#![doc = include_str!("../README.md")]

pub mod adapters;
mod codec;
mod de;
mod field;
mod key;
mod object;
mod raw;
mod reader;

pub use brz_json_macros::model;
pub use codec::{from_slice, from_str, from_value, to_string, to_value, to_vec, to_writer};
pub use field::FieldState;
pub use object::Object;
pub use raw::{
    JsonNull, JsonProjection, OpaqueJson, RawJson, RawJsonRef, raw_json, raw_null, to_raw_value,
};
use serde_json::Value;

#[doc(hidden)]
pub mod __private {
    pub use crate::field::Field;
}

pub use reader::JsonReader;

/// A JSON syntax or conversion error with a zero-based input byte offset.
#[derive(Debug)]
pub struct Error {
    message: String,
    offset: Option<usize>,
    path: Option<String>,
}

impl Error {
    pub(crate) fn at(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
            path: None,
        }
    }

    /// Position relative to the original IO reader, if known.
    pub fn byte_offset(&self) -> Option<usize> {
        self.offset
    }

    /// Typed field path, including array indices, when decoding a model.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub(crate) fn with_path(mut self, path: String) -> Self {
        if path != "." && !path.is_empty() {
            self.path = Some(path);
        }
        self
    }

    pub(crate) fn with_offset(mut self, offset: usize) -> Self {
        if self.offset.is_none() {
            self.offset = Some(offset);
        }
        self
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.offset {
            Some(offset) => write!(f, "{} at byte {}", self.message, offset),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self {
            message: message.to_string(),
            offset: None,
            path: None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            offset: None,
            path: None,
        }
    }
}
