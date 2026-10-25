//! Integer-key maps also deserialize through serde's buffered tagged-enum
//! representation, whose keys arrive as strings rather than JSON key adapters.
use serde::{
    Deserialize, Deserializer,
    de::{Error, MapAccess, Visitor},
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    marker::PhantomData,
    str::FromStr,
};
#[derive(Deserialize)]
#[serde(untagged)]
enum Key {
    Text(String),
    Number(u64),
}
struct Entries<K, V>(PhantomData<(K, V)>);
impl<'de, K: FromStr + TryFrom<u64> + Ord, V: Deserialize<'de>> Visitor<'de> for Entries<K, V> {
    type Value = BTreeMap<K, V>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("integer-key map")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut result = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<Key, V>()? {
            let key = match key {
                Key::Text(text) => text
                    .parse()
                    .map_err(|_| M::Error::custom("Invalid integer key"))?,
                Key::Number(number) => {
                    K::try_from(number).map_err(|_| M::Error::custom("Integer key out of range"))?
                }
            };
            if result.insert(key, value).is_some() {
                return Err(M::Error::custom("Duplicate integer key"));
            }
        }
        Ok(result)
    }
}
pub fn usize_map<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
    de: D,
) -> Result<BTreeMap<usize, V>, D::Error> {
    de.deserialize_map(Entries(PhantomData))
}
pub fn u32_map<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
    de: D,
) -> Result<HashMap<u32, V>, D::Error> {
    de.deserialize_map(Entries(PhantomData))
        .map(|map: BTreeMap<u32, V>| map.into_iter().collect())
}
