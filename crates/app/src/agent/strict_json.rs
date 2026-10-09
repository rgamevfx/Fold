//! Validate opaque provider bodies without interpreting their creative schema.
//! A Value normally loses duplicate keys before the provider can reject them.
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt};
struct Checked;
impl<'de> Deserialize<'de> for Checked {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Self)
    }
}
impl<'de> Visitor<'de> for Checked {
    type Value = Self;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON without duplicate object keys")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self, A::Error> {
        while seq.next_element::<Checked>()?.is_some() {}
        Ok(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self, A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("Duplicate JSON object key"));
            }
            map.next_value::<Checked>()?;
        }
        Ok(self)
    }
}
pub(super) fn validate(bytes: &[u8]) -> Result<(), serde_json::Error> {
    serde_json::from_slice::<Checked>(bytes).map(|_| ())
}
