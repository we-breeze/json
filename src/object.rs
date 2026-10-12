use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An insertion-ordered JSON object with typed values. Repeated keys replace
/// their value while retaining the original position, matching JSON maps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<T>(Vec<(String, T)>);

impl<T> Default for Object<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<T> Object<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }
    pub fn get(&self, key: &str) -> Option<&T> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
    pub fn insert(&mut self, key: String, value: T) -> Option<T> {
        if let Some((_, old)) = self.0.iter_mut().find(|(name, _)| name == &key) {
            return Some(std::mem::replace(old, value));
        }
        self.0.push((key, value));
        None
    }
    pub fn remove(&mut self, key: &str) -> Option<T> {
        let index = self.0.iter().position(|(name, _)| name == key)?;
        Some(self.0.remove(index).1)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut T) -> bool) {
        self.0.retain_mut(|(key, value)| keep(key, value));
    }
}
impl<T> FromIterator<(String, T)> for Object<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(iter: I) -> Self {
        let mut object = Self::new();
        for (key, value) in iter {
            object.insert(key, value);
        }
        object
    }
}
impl<T> IntoIterator for Object<T> {
    type Item = (String, T);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut object = Object::new();
                while let Some((key, value)) = map.next_entry()? {
                    object.insert(key, value);
                }
                Ok(object)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
    }
}
impl<T: Serialize> Serialize for Object<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}
