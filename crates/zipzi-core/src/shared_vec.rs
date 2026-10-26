//! Shared persistent sequences. Cloning a descriptor does not copy its payload;
//! mutations detach the vector so checkpoints and readers remain immutable.
use serde::{Deserialize, Serialize};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SharedVec<T>(Arc<Vec<T>>);

impl<T: crate::api_shape::ApiShape> crate::api_shape::ApiShape for SharedVec<T> {
    fn api_shape() -> crate::api_shape::Shape {
        <Vec<T> as crate::api_shape::ApiShape>::api_shape()
    }
}
impl<T: crate::binary_wire::WireWrite> crate::binary_wire::WireWrite for SharedVec<T> {
    const SIGNATURE: u128 = <Vec<T> as crate::binary_wire::WireWrite>::SIGNATURE;
    fn encode<W: std::io::Write>(&self, out: &mut W) -> crate::binary_wire::Result<()> {
        self.0.encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        self.0.expansion_bytes()
    }
}
impl<T: crate::binary_wire::WireRead> crate::binary_wire::WireRead for SharedVec<T> {
    fn input_cost(input: &mut crate::binary_wire::Reader<'_>) -> crate::binary_wire::Result<usize> {
        <Vec<T> as crate::binary_wire::WireRead>::input_cost(input)
    }
    fn decode(input: &mut crate::binary_wire::Reader<'_>) -> crate::binary_wire::Result<Self> {
        Ok(<Vec<T> as crate::binary_wire::WireRead>::decode(input)?.into())
    }
}

impl<T> Clone for SharedVec<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Default for SharedVec<T> {
    fn default() -> Self {
        Self(Arc::new(Vec::new()))
    }
}
impl<T> From<Vec<T>> for SharedVec<T> {
    fn from(values: Vec<T>) -> Self {
        Self(Arc::new(values))
    }
}
impl<T> FromIterator<T> for SharedVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Vec::from_iter(values).into()
    }
}
impl<T> Deref for SharedVec<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: Clone> DerefMut for SharedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}
impl<'a, T> IntoIterator for &'a SharedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: Clone> IntoIterator for &'a mut SharedVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
impl<T: Clone> IntoIterator for SharedVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        Arc::unwrap_or_clone(self.0).into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_roundtrip_uses_vec_layout_and_admits_before_allocating() {
        use crate::binary_wire::{Reader, WireRead, WireWrite};
        let original: SharedVec<String> = vec!["first".into(), "second".into()].into();
        let mut bytes = Vec::new();
        original.encode(&mut bytes).unwrap();
        let mut plain = Vec::new();
        original.to_vec().encode(&mut plain).unwrap();
        assert_eq!(bytes, plain);
        let mut cost = Reader::new(&bytes).unwrap();
        assert!(SharedVec::<String>::input_cost(&mut cost).unwrap() >= 11);
        cost.finish().unwrap();
        let mut input = Reader::new(&bytes).unwrap();
        let decoded = SharedVec::<String>::decode(&mut input).unwrap();
        input.finish().unwrap();
        assert_eq!(decoded, original);
        let mut changed = decoded.clone();
        changed[0] = "new".into();
        assert_eq!(decoded[0], "first");
        let invalid = u32::MAX.to_le_bytes();
        let mut oversized = Reader::new(&invalid).unwrap();
        assert!(SharedVec::<String>::input_cost(&mut oversized).is_err());
    }
    #[test]
    fn clones_share_storage_and_mutations_detach_without_changing_json() {
        let original: SharedVec<String> = vec!["old".into()].into();
        let mut candidate = original.clone();
        assert_eq!(original.as_ptr(), candidate.as_ptr());
        candidate[0] = "new".into();
        assert_ne!(original.as_ptr(), candidate.as_ptr());
        assert_eq!(original[0], "old");
        assert_eq!(
            serde_json::to_value(&candidate).unwrap(),
            serde_json::json!(["new"])
        );
        let restored: SharedVec<String> =
            serde_json::from_value(serde_json::json!(["new"])).unwrap();
        assert_eq!(restored, candidate);
    }
}
