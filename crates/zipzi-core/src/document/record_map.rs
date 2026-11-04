//! Snapshot copies share records; mutating a record detaches only that record.
use std::{
    collections::HashMap,
    ops::{Deref, Index},
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RecordMap<T>(HashMap<String, Arc<T>>);

impl<T> Default for RecordMap<T> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<T> Deref for RecordMap<T> {
    type Target = HashMap<String, Arc<T>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromIterator<(String, T)> for RecordMap<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(records: I) -> Self {
        Self(
            records
                .into_iter()
                .map(|(id, value)| (id, Arc::new(value)))
                .collect(),
        )
    }
}

impl<T> RecordMap<T> {
    pub fn get(&self, id: &str) -> Option<&T> {
        self.0.get(id).map(Arc::as_ref)
    }
    pub fn insert(&mut self, id: String, value: T) {
        self.0.insert(id, Arc::new(value));
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&String, &T) -> bool) {
        self.0.retain(|id, value| keep(id, value.as_ref()));
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &T)> {
        self.0.iter().map(|(id, value)| (id, value.as_ref()))
    }
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.0.values().map(Arc::as_ref)
    }
}

impl<T: Clone> RecordMap<T> {
    pub fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        self.0.get_mut(id).map(Arc::make_mut)
    }
    pub fn remove(&mut self, id: &str) -> Option<T> {
        self.0.remove(id).map(Arc::unwrap_or_clone)
    }
    pub fn retain_mut(&mut self, mut keep: impl FnMut(&String, &mut T) -> bool) {
        self.0.retain(|id, value| keep(id, Arc::make_mut(value)));
    }
}

impl<T, Q: AsRef<str>> Index<Q> for RecordMap<T> {
    type Output = T;
    fn index(&self, id: Q) -> &T {
        &self.0[id.as_ref()]
    }
}

impl<'a, T> IntoIterator for &'a RecordMap<T> {
    type Item = (&'a String, &'a T);
    type IntoIter = std::iter::Map<
        std::collections::hash_map::Iter<'a, String, Arc<T>>,
        fn((&'a String, &'a Arc<T>)) -> (&'a String, &'a T),
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(id, value)| (id, value.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_detach_only_the_written_record() {
        let mut original = RecordMap::default();
        original.insert("a".into(), vec![1, 2]);
        original.insert("b".into(), vec![3, 4]);
        let mut snapshot = original.clone();
        assert!(std::ptr::eq(
            original.get("a").unwrap(),
            snapshot.get("a").unwrap()
        ));
        snapshot.get_mut("a").unwrap().push(5);
        assert_eq!(original.get("a").unwrap(), &[1, 2]);
        assert_eq!(snapshot.get("a").unwrap(), &[1, 2, 5]);
        assert!(std::ptr::eq(
            original.get("b").unwrap(),
            snapshot.get("b").unwrap()
        ));
        snapshot.retain(|id, _| id == "b");
        assert!(std::ptr::eq(
            original.get("b").unwrap(),
            snapshot.get("b").unwrap()
        ));
        snapshot.remove("b");
        assert!(original.contains_key("b"));
    }
}
