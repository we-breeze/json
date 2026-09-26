//! Decode a bounded subrange of a receive Reader, without copying its Body.
use super::{Cell, Input, JsonReader, ReaderView};

impl<'a> JsonReader<'a> {
    /// Parse only this view using an independent cursor. In particular, HTTP
    /// framing and any pipelined next request remain outside the JSON input.
    /// The source Reader owns cross-segment/escaped derived data.
    pub fn from_view(input: ReaderView<'a>) -> Self {
        Self {
            input: Input::Borrowed(input),
            cursor: Cell::new(0),
            depth: Cell::new(0),
            max_depth: 128,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn bounded_body_excludes_framing_and_pipelined_data() {
        let arena = brz_io_arena();
        let mut writer = brz_io::Writer::new(&arena);
        writer.write_all(b"PREFIX\"hello\"NEXT").unwrap();
        let reader = writer.into_reader();
        let body = reader.view().slice(6..13).unwrap();
        let json = JsonReader::from_view(body);
        let text: &str = json.decode().unwrap();
        assert_eq!(text, "hello");
        assert_eq!(
            text.as_ptr(),
            reader.view().peek_bytes(7, 5).unwrap().as_ptr()
        );
        assert_eq!(reader.position(), 0);
        assert!(
            JsonReader::from_view(reader.view())
                .decode::<&str>()
                .is_err()
        );
    }

    // brz-json already uses brz-io, but need no new direct brz-ds dependency:
    // expose the arena type through brz-io's new re-export.
    fn brz_io_arena() -> brz_io::EphemeralBytesArena {
        brz_io::EphemeralBytesArena::new(4096)
    }

    #[test]
    fn cross_segment_and_escaped_strings_keep_borrowed_results_alive() {
        let arena = brz_io::EphemeralBytesArena::new(3);
        let mut writer = brz_io::Writer::new(&arena);
        let raw = br#"["a\u0062","xyz"]"#;
        writer.write_all(raw).unwrap();
        writer.write_all(b"NEXT").unwrap();
        let reader = writer.into_reader();
        let json = JsonReader::from_view(reader.view().slice(0..raw.len()).unwrap());
        let (first, second): (&str, &str) = json.decode().unwrap();
        assert_eq!((first, second), ("ab", "xyz"));
        assert_eq!(reader.position(), 0);
        assert_eq!(first, "ab");
    }
}
