use crate::{Result, Value};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use serde_json::value::RawValue;

pub fn to_raw_value<T: ?Sized + Serialize>(value: &T) -> Result<Box<RawValue>> {
    serde_json::value::to_raw_value(value).map_err(Into::into)
}

pub fn raw_json<T: ?Sized + Serialize>(value: &T) -> Box<RawValue> {
    to_raw_value(value).expect("JSON value serializes")
}

/// A protocol field whose JSON value is always null.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonNull;

pub fn raw_null() -> Box<RawValue> {
    to_raw_value(&JsonNull).expect("JSON null serializes")
}

/// Validated, normalized JSON at legacy boundaries. Unlike `RawValue`, decoding
/// retains Value's number validation and normalization before freezing it.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct OpaqueJson(Box<RawValue>);

impl<'de> Deserialize<'de> for OpaqueJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Value::deserialize(deserializer).map(Self::from)
    }
}
impl From<Value> for OpaqueJson {
    fn from(value: Value) -> Self {
        Self(raw_json(&value))
    }
}
impl OpaqueJson {
    pub fn try_from_serializable(value: impl Serialize) -> Result<Self> {
        to_raw_value(&value).map(Self)
    }
    /// Convenience for known infallible application payloads. Use
    /// `try_from_serializable` when the input serializer can fail.
    pub fn from_serializable(value: impl Serialize) -> Self {
        Self::try_from_serializable(value).expect("JSON value serializes")
    }
    /// Capture valid raw text for later typed decoding, without normalization.
    pub fn from_json_text(text: &str) -> Option<Self> {
        RawValue::from_string(text.to_owned()).ok().map(Self)
    }
    pub fn as_raw(&self) -> &RawValue {
        &self.0
    }
    pub fn is_null(&self) -> bool {
        self.0.get() == "null"
    }
    pub fn decode<'de, T: Deserialize<'de>>(&'de self) -> Result<T> {
        crate::from_str(self.0.get())
    }
    /// Explicitly tolerant typed projection. Syntax was validated at capture.
    pub fn project<T: DeserializeOwned>(&self) -> Option<T> {
        self.decode().ok()
    }
    pub fn to_value(&self) -> Value {
        crate::from_str(self.0.get()).expect("opaque JSON is representable as a value")
    }
    pub fn to_raw_value(&self) -> Box<RawValue> {
        self.0.clone()
    }
    pub fn is_nonempty_object(&self) -> bool {
        crate::from_str::<crate::Object<serde::de::IgnoredAny>>(self.0.get())
            .is_ok_and(|object| !object.is_empty())
    }
}

/// A tolerant typed projection: malformed JSON/numbers still fail decoding;
/// valid JSON that cannot decode as T produces an absent projection.
#[derive(Debug, Clone)]
pub struct JsonProjection<T> {
    pub value: Option<T>,
}
impl<T: DeserializeOwned> From<Value> for JsonProjection<T> {
    fn from(value: Value) -> Self {
        Self {
            value: crate::from_value(value).ok(),
        }
    }
}
impl<'de, T: DeserializeOwned> Deserialize<'de> for JsonProjection<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Value::deserialize(deserializer).map(Self::from)
    }
}
impl<T: DeserializeOwned> JsonProjection<T> {
    pub fn from_json(value: &Value) -> Self {
        Self {
            value: T::deserialize(value).ok(),
        }
    }
}
