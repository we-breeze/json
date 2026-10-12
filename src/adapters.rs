//! Explicit, opt-in field conversion policies. JSON syntax errors always fail.
//! Missing fields are handled separately by `#[serde(default)]` on the model.
use serde::de::{self, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

pub fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Attempt<T> {
    Typed(T),
    Other(IgnoredAny),
}

/// Consume one complete value before tolerating a type mismatch. This avoids
/// swallowing an error halfway through a streaming deserializer.
pub fn try_decode<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(match Attempt::<T>::deserialize(deserializer)? {
        Attempt::Typed(value) => Some(value),
        Attempt::Other(_) => None,
    })
}
pub fn invalid_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(try_decode(deserializer)?.unwrap_or_default())
}

/// Require an object even when T's Serde implementation accepts positional arrays.
pub fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

struct ObjectItem<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectItem<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        object(deserializer).map(Self)
    }
}
pub fn optional_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<ObjectItem<T>>::deserialize(deserializer)?.map(|item| item.0))
}
pub fn optional_objects<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<ObjectItem<T>>>::deserialize(deserializer)?
        .map(|items| items.into_iter().map(|item| item.0).collect()))
}

/// Keep nonempty strings; null maps to None and other types are rejected.
pub fn nonempty_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.filter(|value| !value.is_empty()))
}

/// Python/Pydantic-style integer coercion: bools, integer strings and integral
/// floats are accepted. Fractional floats and out-of-range values are errors.
pub fn integer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    integer_with::<D, false>(deserializer)
}
/// Explicit Python int-style truncation, for legacy consumers that require it.
pub fn truncated_integer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    integer_with::<D, true>(deserializer)
}
fn integer_with<'de, D: Deserializer<'de>, const TRUNCATE: bool>(
    deserializer: D,
) -> Result<i64, D::Error> {
    struct IntVisitor<const TRUNCATE: bool>;
    impl<'de, const TRUNCATE: bool> Visitor<'de> for IntVisitor<TRUNCATE> {
        type Value = i64;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a convertible integer")
        }
        fn visit_bool<E: Error>(self, value: bool) -> Result<i64, E> {
            Ok(i64::from(value))
        }
        fn visit_i64<E: Error>(self, value: i64) -> Result<i64, E> {
            Ok(value)
        }
        fn visit_u64<E: Error>(self, value: u64) -> Result<i64, E> {
            value
                .try_into()
                .map_err(|_| E::custom("integer out of range"))
        }
        fn visit_f64<E: Error>(self, value: f64) -> Result<i64, E> {
            const BOUND: f64 = 9_223_372_036_854_775_808.0;
            if value.is_finite()
                && (-BOUND..BOUND).contains(&value)
                && (TRUNCATE || value != -BOUND)
                && (TRUNCATE || value.fract() == 0.0)
            {
                Ok(value as i64)
            } else {
                Err(E::custom("expected an in-range integer"))
            }
        }
        fn visit_str<E: Error>(self, value: &str) -> Result<i64, E> {
            let value = value.trim();
            if let Ok(value) = value.parse() {
                return Ok(value);
            }
            if !TRUNCATE
                && let Some((whole, fraction)) = value.split_once('.')
                && !fraction.is_empty()
                && fraction.bytes().all(|byte| byte == b'0')
                && let Ok(value) = whole.parse()
            {
                return Ok(value);
            }
            Err(E::custom("expected an integer string"))
        }
    }
    deserializer.deserialize_any(IntVisitor::<TRUNCATE>)
}

pub fn number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    struct NumberVisitor;
    impl<'de> Visitor<'de> for NumberVisitor {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a convertible number")
        }
        fn visit_bool<E: Error>(self, value: bool) -> Result<f64, E> {
            Ok(if value { 1.0 } else { 0.0 })
        }
        fn visit_i64<E: Error>(self, value: i64) -> Result<f64, E> {
            Ok(value as f64)
        }
        fn visit_u64<E: Error>(self, value: u64) -> Result<f64, E> {
            Ok(value as f64)
        }
        fn visit_f64<E: Error>(self, value: f64) -> Result<f64, E> {
            Ok(value)
        }
        fn visit_str<E: Error>(self, value: &str) -> Result<f64, E> {
            value
                .trim()
                .parse()
                .map_err(|_| E::custom("expected a number string"))
        }
    }
    deserializer.deserialize_any(NumberVisitor)
}

/// Boolean coercion accepts only 0/1 and the documented spellings. It does
/// not use Python truthiness: for example, the string "false" maps to false.
pub fn boolean<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    boolean_impl(deserializer)
}
/// Legacy responses policy: any signed integer, trimmed spellings and an empty string.
pub fn legacy_boolean<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    // Preserve the legacy diagnostic as well as its conversions: applications
    // may expose this error text at an HTTP response boundary.
    match crate::Value::deserialize(deserializer)? {
        crate::Value::Bool(value) => Ok(value),
        crate::Value::Number(value) => value
            .as_i64()
            .map(|value| value != 0)
            .ok_or_else(|| D::Error::custom("expected a boolean")),
        crate::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" | "" => Ok(false),
            _ => Err(D::Error::custom("expected a boolean")),
        },
        _ => Err(D::Error::custom("expected a boolean")),
    }
}
fn boolean_impl<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    struct BoolVisitor;
    impl<'de> Visitor<'de> for BoolVisitor {
        type Value = bool;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a convertible boolean")
        }
        fn visit_bool<E: Error>(self, value: bool) -> Result<bool, E> {
            Ok(value)
        }
        fn visit_i64<E: Error>(self, value: i64) -> Result<bool, E> {
            match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(E::custom("expected 0 or 1")),
            }
        }
        fn visit_u64<E: Error>(self, value: u64) -> Result<bool, E> {
            match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(E::custom("expected 0 or 1")),
            }
        }
        fn visit_f64<E: Error>(self, value: f64) -> Result<bool, E> {
            if value == 0.0 {
                Ok(false)
            } else if value == 1.0 {
                Ok(true)
            } else {
                Err(E::custom("expected a boolean"))
            }
        }
        fn visit_str<E: Error>(self, value: &str) -> Result<bool, E> {
            match value.to_ascii_lowercase().as_str() {
                "0" | "off" | "false" | "no" => Ok(false),
                "1" | "on" | "true" | "yes" => Ok(true),
                "f" | "n" => Ok(false),
                "t" | "y" => Ok(true),
                _ => Err(E::custom("expected a boolean spelling")),
            }
        }
    }
    deserializer.deserialize_any(BoolVisitor)
}

macro_rules! optional {
    ($name:ident, $ty:ty, $decode:ident) => {
        pub fn $name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<$ty>, D::Error> {
            struct Item($ty);
            impl<'de> Deserialize<'de> for Item {
                fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    $decode(deserializer).map(Self)
                }
            }
            Ok(Option::<Item>::deserialize(deserializer)?.map(|value| value.0))
        }
    };
}
optional!(optional_integer, i64, integer);
optional!(optional_number, f64, number);
optional!(optional_boolean, bool, boolean);
optional!(optional_str, String, required_str);
optional!(optional_strings, Vec<String>, strings);
optional!(optional_integers, Vec<i64>, integers);

pub fn required_str<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    String::deserialize(deserializer)
}
pub fn strings<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Vec::deserialize(deserializer)
}
pub fn integers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<i64>, D::Error> {
    #[derive(Deserialize)]
    struct Item(#[serde(deserialize_with = "integer")] i64);
    Ok(Vec::<Item>::deserialize(deserializer)?
        .into_iter()
        .map(|value| value.0)
        .collect())
}

/// Remove null entries, rejecting other invalid elements. This is distinct
/// from filtering all type mismatches.
pub fn nonnull_items<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Vec::<Option<T>>::deserialize(deserializer)?
        .into_iter()
        .flatten()
        .collect())
}

/// Python bool(value), explicitly opted into for legacy protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonTruthiness(pub bool);
impl<'de> Deserialize<'de> for PythonTruthiness {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TruthVisitor;
        impl<'de> Visitor<'de> for TruthVisitor {
            type Value = PythonTruthiness;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
                Ok(PythonTruthiness(false))
            }
            fn visit_bool<E: Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(PythonTruthiness(value))
            }
            fn visit_i64<E: Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(PythonTruthiness(value != 0))
            }
            fn visit_u64<E: Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(PythonTruthiness(value != 0))
            }
            fn visit_f64<E: Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(PythonTruthiness(value != 0.0))
            }
            fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(PythonTruthiness(!value.is_empty()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut populated = false;
                while seq.next_element::<IgnoredAny>()?.is_some() {
                    populated = true;
                }
                Ok(PythonTruthiness(populated))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut populated = false;
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
                    populated = true;
                }
                Ok(PythonTruthiness(populated))
            }
        }
        deserializer.deserialize_any(TruthVisitor)
    }
}

pub fn python_truthy(value: &crate::Value) -> bool {
    use crate::Value;
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
