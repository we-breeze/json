//! Borrow JSON strings from segmented arena storage without an intermediate DOM.
//!
//! `&JsonReader` implements `serde::Deserializer<'de>`. Keep the reader alive
//! for as long as deserialized fields are borrowed. A successful decode consumes
//! input; use a new reader to parse the document again.
//!
//! ```
//! use brz_ds::EphemeralBytesArena;
//! use brz_io::Writer;
//! use brz_json::JsonReader;
//! use serde::Deserialize;
//! use std::io::Write;
//!
//! #[derive(Deserialize)]
//! struct User<'a> { name: &'a str }
//! let arena = EphemeralBytesArena::new(1024);
//! let mut writer = Writer::new(&arena);
//! writer.write_all(br#"{"name":"a\u0062"}"#)?;
//! let json = JsonReader::new(writer.into_reader());
//! let user: User<'_> = json.decode()?;
//! assert_eq!(user.name, "ab");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod de;
mod key;
mod reader;

pub use reader::JsonReader;

/// A JSON syntax or conversion error with a zero-based input byte offset.
#[derive(Debug)]
pub struct Error {
    message: String,
    offset: Option<usize>,
}

impl Error {
    pub(crate) fn at(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
        }
    }

    /// Position relative to the original IO reader, if known.
    pub fn byte_offset(&self) -> Option<usize> {
        self.offset
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
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
