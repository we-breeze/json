use super::JsonReader;
use crate::Result;

impl JsonReader<'_> {
    /// Validate and capture only this JSON value. Adjacent framing remains out
    /// of scope. The Reader owns coalesced ranges, including borrowed results.
    pub(crate) fn raw_value(&self) -> Result<&str> {
        self.whitespace()?;
        let start = self.position();
        self.skip_raw_value()?;
        let bytes = self
            .input()
            .peek_bytes(start, self.position() - start)
            .map_err(|error| self.error(error.to_string()))?;
        std::str::from_utf8(bytes).map_err(|error| self.error(error.to_string()))
    }

    fn skip_raw_value(&self) -> Result<()> {
        self.whitespace()?;
        match self.peek() {
            Some(b'n') => self.literal(b"null"),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'"') => self.string().map(|_| ()),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.position();
                let number = self.number()?;
                // Raw JSON preserves valid numeric spellings even when their
                // magnitude cannot be represented by Value's finite f64.
                serde_json::from_str::<&serde_json::value::RawValue>(number)
                    .map(|_| ())
                    .map_err(|error| crate::Error::at(error.to_string(), start))
            }
            Some(b'[') => {
                let _depth = self.enter()?;
                self.expect(b'[')?;
                self.whitespace()?;
                if self.peek() != Some(b']') {
                    loop {
                        self.skip_raw_value()?;
                        self.whitespace()?;
                        if self.peek() == Some(b']') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                self.expect(b']')
            }
            Some(b'{') => {
                let _depth = self.enter()?;
                self.expect(b'{')?;
                self.whitespace()?;
                if self.peek() != Some(b'}') {
                    loop {
                        self.string()?;
                        self.expect(b':')?;
                        self.skip_raw_value()?;
                        self.whitespace()?;
                        if self.peek() == Some(b'}') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                self.expect(b'}')
            }
            _ => Err(self.error("expected a JSON value")),
        }
    }
}
