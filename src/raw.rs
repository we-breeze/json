use crate::{Result, Value};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use serde_json::value::RawValue;

/// An owned, validated JSON fragment. Serialization emits the JSON itself,
/// preserving its spelling rather than quoting it or building a dynamic tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawJson(Box<RawValue>);

impl RawJson {
    pub fn from_string(text: String) -> Result<Self> {
        RawValue::from_string(text).map(Self).map_err(Into::into)
    }
    pub fn get(&self) -> &str {
        self.0.get()
    }
    pub fn is_null(&self) -> bool {
        self.get() == "null"
    }
    pub fn decode<'de, T: Deserialize<'de>>(&'de self) -> Result<T> {
        crate::from_str(self.get())
    }
    pub fn as_borrowed(&self) -> RawJsonRef<'_> {
        RawJsonRef(&self.0)
    }
}

/// A validated JSON fragment borrowed from contiguous or segmented input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawJsonRef<'a>(#[serde(borrow)] &'a RawValue);

impl<'a> RawJsonRef<'a> {
    pub fn get(&self) -> &'a str {
        self.0.get()
    }
    pub fn is_null(&self) -> bool {
        self.get() == "null"
    }
    pub fn decode<T: Deserialize<'a>>(&self) -> Result<T> {
        crate::from_str(self.get())
    }
    pub fn to_owned(&self) -> RawJson {
        RawJson(self.0.to_owned())
    }
}

pub fn to_raw_value<T: ?Sized + Serialize>(value: &T) -> Result<RawJson> {
    serde_json::value::to_raw_value(value)
        .map(RawJson)
        .map_err(Into::into)
}

pub fn raw_json<T: ?Sized + Serialize>(value: &T) -> RawJson {
    to_raw_value(value).expect("JSON value serializes")
}

/// A protocol field whose JSON value is always null.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonNull;

pub fn raw_null() -> RawJson {
    to_raw_value(&JsonNull).expect("JSON null serializes")
}

/// Validated, normalized JSON at legacy boundaries. Unlike `RawValue`, decoding
/// retains Value's number validation and normalization before freezing it.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct OpaqueJson(RawJson);

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
        RawJson::from_string(text.to_owned()).ok().map(Self)
    }
    pub fn as_raw(&self) -> &RawJson {
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
    pub fn to_raw_value(&self) -> RawJson {
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
