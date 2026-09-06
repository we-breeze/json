use std::cell::Cell;

use brz_io::{Reader, ReaderView};
use serde::Deserialize;

use crate::{Error, Result};

/// A JSON deserializer owning or borrowing segmented input.
///
/// Unescaped strings in one segment borrow the input directly. Cross-segment
/// strings and decoded escape sequences live in the input reader's arena.
/// Reading later fields never invalidates earlier borrowed fields.
///
/// Decode is synchronous; the holder can be moved into an async task and kept
/// alive across awaits. It is intentionally not Sync: one parser owns its cursor.
#[derive(Debug)]
pub struct JsonReader<'a> {
    input: Input<'a>,
    cursor: Cell<usize>,
    depth: Cell<usize>,
    max_depth: usize,
}

#[derive(Debug)]
enum Input<'a> {
    Owned(Reader),
    Borrowed(ReaderView<'a>),
}

impl JsonReader<'static> {
    pub fn new(input: Reader) -> Self {
        Self {
            input: Input::Owned(input),
            cursor: Cell::new(0),
            depth: Cell::new(0),
            max_depth: 128,
        }
    }

    /// Set the maximum nesting of arrays, objects, and externally tagged enums.
    pub fn with_recursion_limit(input: Reader, max_depth: usize) -> Self {
        Self {
            input: Input::Owned(input),
            cursor: Cell::new(0),
            depth: Cell::new(0),
            max_depth,
        }
    }
}

impl<'a> JsonReader<'a> {
    /// Parse a snapshot of unread input with an independent cursor. Neither
    /// parsing nor later shared reads change the other reader's position.
    pub fn from_borrowed(input: &'a Reader) -> Self {
        Self {
            input: Input::Borrowed(input.view()),
            cursor: Cell::new(0),
            depth: Cell::new(0),
            max_depth: 128,
        }
    }

    fn input(&self) -> ReaderView<'_> {
        match &self.input {
            Input::Owned(reader) => reader.view(),
            Input::Borrowed(view) => *view,
        }
    }

    /// Decode exactly one JSON document and reject trailing non-whitespace.
    /// Errors can consume a prefix; discard the reader after a failed decode.
    pub fn decode<'de, T: Deserialize<'de>>(&'de self) -> Result<T> {
        let value = T::deserialize(self).map_err(|error| error.with_offset(self.position()))?;
        self.end()?;
        Ok(value)
    }

    /// Check the end after using `T::deserialize(&json)` directly.
    pub fn end(&self) -> Result<()> {
        self.whitespace()?;
        if self.peek().is_some() {
            Err(self.error("trailing characters"))
        } else {
            Ok(())
        }
    }

    pub fn position(&self) -> usize {
        self.cursor.get()
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> Error {
        Error::at(message, self.position())
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.peek_byte(0)
    }

    fn peek_byte(&self, offset: usize) -> Option<u8> {
        self.input().peek_byte(self.position().checked_add(offset)?)
    }

    fn read_str(&self, len: usize) -> Result<&str> {
        let bytes = self
            .input()
            .peek_bytes(self.position(), len)
            .map_err(|error| self.error(error.to_string()))?;
        let value = std::str::from_utf8(bytes).map_err(|error| self.error(error.to_string()))?;
        self.skip(len)?;
        Ok(value)
    }

    pub(crate) fn skip(&self, count: usize) -> Result<()> {
        let next = self
            .position()
            .checked_add(count)
            .filter(|next| *next <= self.input().len())
            .ok_or_else(|| self.error("unexpected end of JSON"))?;
        self.cursor.set(next);
        Ok(())
    }

    pub(crate) fn whitespace(&self) -> Result<()> {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.skip(1)?;
        }
        Ok(())
    }

    pub(crate) fn expect(&self, byte: u8) -> Result<()> {
        self.whitespace()?;
        if self.peek() != Some(byte) {
            return Err(self.error(format!("expected '{}'", char::from(byte))));
        }
        self.skip(1)
    }

    pub(crate) fn literal(&self, literal: &[u8]) -> Result<()> {
        self.whitespace()?;
        for (index, expected) in literal.iter().enumerate() {
            if self.peek_byte(index) != Some(*expected) {
                return Err(self.error("invalid JSON literal"));
            }
        }
        self.skip(literal.len())
    }

    pub(crate) fn enter(&self) -> Result<DepthGuard<'_>> {
        let depth = self.depth.get();
        if depth >= self.max_depth {
            return Err(self.error("recursion limit exceeded"));
        }
        self.depth.set(depth + 1);
        Ok(DepthGuard(&self.depth))
    }

    pub(crate) fn number(&self) -> Result<&str> {
        self.whitespace()?;
        if !matches!(self.peek(), Some(b'-' | b'0'..=b'9')) {
            return Err(self.error("expected a number"));
        }
        let mut len = 0;
        while matches!(
            self.peek_byte(len),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            len += 1;
        }
        self.read_str(len)
    }

    pub(crate) fn string(&self) -> Result<&str> {
        self.expect(b'"')?;
        let start = self.position();
        let mut len = 0;
        let mut escaped = false;
        loop {
            match self.peek_byte(len) {
                Some(b'"') => break,
                Some(b'\\') => {
                    escaped = true;
                    len += 2;
                }
                Some(0..=0x1f) => {
                    return Err(Error::at("control character in JSON string", start + len));
                }
                Some(_) => len += 1,
                None => return Err(Error::at("unterminated JSON string", start + len)),
            }
        }
        let value = if escaped {
            let bytes = self.input().store_bytes_with(len, |output| {
                let mut index = 0;
                while index < len {
                    let byte = self.peek_byte(index).expect("scanned string");
                    index += 1;
                    if byte != b'\\' {
                        if byte < 0x20 {
                            return Err(Error::at(
                                "control character in JSON string",
                                start + index - 1,
                            ));
                        }
                        output.extend_from_slice(&[byte]);
                        continue;
                    }
                    let escape = self
                        .peek_byte(index)
                        .ok_or_else(|| self.error("incomplete escape"))?;
                    index += 1;
                    match escape {
                        b'"' | b'\\' | b'/' => output.extend_from_slice(&[escape]),
                        b'b' => output.extend_from_slice(&[8]),
                        b'f' => output.extend_from_slice(&[12]),
                        b'n' => output.extend_from_slice(b"\n"),
                        b'r' => output.extend_from_slice(b"\r"),
                        b't' => output.extend_from_slice(b"\t"),
                        b'u' => {
                            let high = self.hex(index, len)?;
                            index += 4;
                            let code = if (0xd800..=0xdbff).contains(&high) {
                                if self.peek_byte(index) != Some(b'\\')
                                    || self.peek_byte(index + 1) != Some(b'u')
                                {
                                    return Err(Error::at("missing low surrogate", start + index));
                                }
                                index += 2;
                                let low = self.hex(index, len)?;
                                index += 4;
                                if !(0xdc00..=0xdfff).contains(&low) {
                                    return Err(Error::at(
                                        "invalid low surrogate",
                                        start + index - 4,
                                    ));
                                }
                                0x10000 + ((high - 0xd800) << 10) + low - 0xdc00
                            } else {
                                high
                            };
                            let character = char::from_u32(code).ok_or_else(|| {
                                Error::at("invalid Unicode scalar", start + index - 4)
                            })?;
                            output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        _ => return Err(Error::at("invalid JSON escape", start + index - 1)),
                    }
                }
                // Validate before publishing any derived bytes to callers.
                std::str::from_utf8(output.as_slice())
                    .map_err(|error| Error::at(error.to_string(), start))?;
                Ok(())
            })?;
            self.skip(len)?;
            std::str::from_utf8(bytes).map_err(|error| Error::at(error.to_string(), start))?
        } else {
            self.read_str(len)?
        };
        self.skip(1)?; // Closing quote, already located by the scanner.
        Ok(value)
    }

    fn hex(&self, start: usize, limit: usize) -> Result<u32> {
        if start.checked_add(4).is_none_or(|end| end > limit) {
            return Err(self.error("incomplete Unicode escape"));
        }
        let mut value = 0;
        for index in start..start + 4 {
            let digit = self
                .peek_byte(index)
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or_else(|| Error::at("invalid Unicode escape", self.position() + index))?;
            value = value * 16 + digit;
        }
        Ok(value)
    }
}

pub(crate) struct DepthGuard<'a>(&'a Cell<usize>);
impl Drop for DepthGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
