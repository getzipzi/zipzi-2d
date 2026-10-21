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
