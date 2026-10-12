use crate::{Error, Result, Value};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

fn decode<'de, T: Deserialize<'de>>(
    de: &mut serde_json::Deserializer<impl serde_json::de::Read<'de>>,
) -> Result<T> {
    let value = serde_path_to_error::deserialize(&mut *de).map_err(|error| {
        let path = error.path().to_string();
        Error::from(error.into_inner()).with_path(path)
    })?;
    de.end()?;
    Ok(value)
}

/// Decode exactly one document, retaining Serde's borrowing from the input.
pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T> {
    decode(&mut serde_json::Deserializer::from_str(input))
}

pub fn from_slice<'de, T: Deserialize<'de>>(input: &'de [u8]) -> Result<T> {
    decode(&mut serde_json::Deserializer::from_slice(input))
}

pub fn from_value<T: DeserializeOwned>(input: Value) -> Result<T> {
    serde_path_to_error::deserialize(input).map_err(|error| {
        let path = error.path().to_string();
        Error::from(error.into_inner()).with_path(path)
    })
}

pub fn to_string<T: ?Sized + Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(Into::into)
}
pub fn to_vec<T: ?Sized + Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(Into::into)
}
pub fn to_writer<W: std::io::Write, T: ?Sized + Serialize>(writer: W, value: &T) -> Result<()> {
    serde_json::to_writer(writer, value).map_err(Into::into)
}
pub fn to_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(Into::into)
}
