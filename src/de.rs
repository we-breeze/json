use serde::Deserializer;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};

use crate::key::Key;
use crate::{Error, JsonReader, Result};

macro_rules! number {
    ($($method:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            self.whitespace()?;
            let start = self.position();
            let text = self.number()?;
            let mut parser = serde_json::Deserializer::from_str(text);
            let value = parser.$method(visitor)
                .map_err(|error| Error::at(error.to_string(), start))?;
            parser.end().map_err(|error| Error::at(error.to_string(), start))?;
            Ok(value)
        }
    )*};
}

impl<'de> Deserializer<'de> for &'de JsonReader<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.whitespace()?;
        match self.peek() {
            Some(b'n') => self.deserialize_unit(visitor),
            Some(b't' | b'f') => self.deserialize_bool(visitor),
            Some(b'"') => self.deserialize_str(visitor),
            Some(b'[') => self.deserialize_seq(visitor),
            Some(b'{') => self.deserialize_map(visitor),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.position();
                let text = self.number()?;
                let mut parser = serde_json::Deserializer::from_str(text);
                let value = parser
                    .deserialize_any(visitor)
                    .map_err(|error| Error::at(error.to_string(), start))?;
                parser
                    .end()
                    .map_err(|error| Error::at(error.to_string(), start))?;
                Ok(value)
            }
            _ => Err(self.error("expected a JSON value")),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.whitespace()?;
        match self.peek() {
            Some(b't') => {
                self.literal(b"true")?;
                visitor.visit_bool(true)
            }
            Some(b'f') => {
                self.literal(b"false")?;
                visitor.visit_bool(false)
            }
            _ => Err(self.error("expected a boolean")),
        }
    }

    // Delegate numeric conversion to serde_json to preserve its overflow and
    // floating-point behavior. Only the number token is made contiguous.
    number!(
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_i128,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_u128,
        deserialize_f32,
        deserialize_f64
    );

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let value = self.string()?;
        let mut chars = value.chars();
        match (chars.next(), chars.next()) {
            (Some(character), None) => visitor.visit_char(character),
            _ => Err(self.error("expected a single character")),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_str(self.string()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.whitespace()?;
        if self.peek() == Some(b'[') {
            self.deserialize_seq(visitor)
        } else {
            visitor.visit_borrowed_bytes(self.string()?.as_bytes())
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.whitespace()?;
        if self.peek() == Some(b'n') {
            self.literal(b"null")?;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.literal(b"null")?;
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let _depth = self.enter()?;
        self.expect(b'[')?;
        let value = visitor.visit_seq(Sequence {
            json: self,
            first: true,
        })?;
        self.expect(b']')?;
        Ok(value)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let _depth = self.enter()?;
        self.expect(b'{')?;
        let mut map = Mapping {
            json: self,
            first: true,
            pending_value: false,
        };
        let value = visitor.visit_map(&mut map)?;
        if map.pending_value {
            return Err(self.error("object value was not consumed"));
        }
        self.expect(b'}')?;
        Ok(value)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.whitespace()?;
        // Match serde_json's support for positional struct representations.
        if self.peek() == Some(b'[') {
            self.deserialize_seq(visitor)
        } else {
            self.deserialize_map(visitor)
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.whitespace()?;
        if self.peek() == Some(b'"') {
            return visitor.visit_enum(de::value::BorrowedStrDeserializer::<Error>::new(
                self.string()?,
            ));
        }
        let _depth = self.enter()?;
        self.expect(b'{')?;
        let variant = self.string()?;
        self.expect(b':')?;
        let value = visitor.visit_enum(Enumeration {
            json: self,
            variant,
        })?;
        self.expect(b'}')?;
        Ok(value)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(de::IgnoredAny)?;
        visitor.visit_unit()
    }
}

struct Sequence<'de> {
    json: &'de JsonReader<'de>,
    first: bool,
}

impl<'de> SeqAccess<'de> for Sequence<'de> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        self.json.whitespace()?;
        if self.json.peek() == Some(b']') {
            return Ok(None);
        }
        if !self.first {
            self.json.expect(b',')?;
        }
        self.first = false;
        seed.deserialize(self.json).map(Some)
    }
}

struct Mapping<'de> {
    json: &'de JsonReader<'de>,
    first: bool,
    pending_value: bool,
}

impl<'de> MapAccess<'de> for &mut Mapping<'de> {
    type Error = Error;
    fn next_key_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.pending_value {
            return Err(self.json.error("object value was not consumed"));
        }
        self.json.whitespace()?;
        if self.json.peek() == Some(b'}') {
            return Ok(None);
        }
        if !self.first {
            self.json.expect(b',')?;
        }
        self.first = false;
        let key = self.json.string()?;
        self.pending_value = true;
        seed.deserialize(Key(key)).map(Some)
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value> {
        if !self.pending_value {
            return Err(self.json.error("object key was not consumed"));
        }
        self.json.expect(b':')?;
        let value = seed.deserialize(self.json)?;
        self.pending_value = false;
        Ok(value)
    }
}

struct Enumeration<'de> {
    json: &'de JsonReader<'de>,
    variant: &'de str,
}

impl<'de> EnumAccess<'de> for Enumeration<'de> {
    type Error = Error;
    type Variant = &'de JsonReader<'de>;
    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, Self::Variant)> {
        let variant = seed.deserialize(Key(self.variant))?;
        Ok((variant, self.json))
    }
}

impl<'de> VariantAccess<'de> for &'de JsonReader<'de> {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        serde::Deserialize::deserialize(self)
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(self)
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        self.deserialize_tuple(len, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_struct("", fields, visitor)
    }
}
