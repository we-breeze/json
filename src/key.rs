use serde::Deserializer;
use serde::de::{self, Visitor};

use crate::{Error, Result};

pub(crate) struct Key<'de>(pub &'de str);

macro_rules! number {
    ($($method:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            if self.0.bytes().any(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r')) {
                return Err(de::Error::custom("whitespace in numeric or boolean object key"));
            }
            let mut parser = serde_json::Deserializer::from_str(self.0);
            let value = parser.$method(visitor).map_err(de::Error::custom)?;
            parser.end().map_err(de::Error::custom)?;
            Ok(value)
        }
    )*};
}

impl<'de> Deserializer<'de> for Key<'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_str(self.0)
    }

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
        deserialize_f64,
        deserialize_bool
    );

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_some(self)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::value::BorrowedStrDeserializer::<Error>::new(self.0)
            .deserialize_enum(name, variants, visitor)
    }

    serde::forward_to_deserialize_any! {
        char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
