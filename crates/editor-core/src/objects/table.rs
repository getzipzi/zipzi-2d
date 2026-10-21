//! Stable slots with independent record snapshots. An edit copies one record,
//! one page of pointers and (when shared) the page directory, not every payload.
use kasane_core::SharedVec;
use std::{
    ops::{Index, IndexMut, Range},
    sync::Arc,
};

const PAGE_SIZE: usize = 32;

#[derive(Clone, Debug)]
pub struct SnapshotTable<T> {
    pages: SharedVec<SharedVec<Arc<T>>>,
    len: usize,
}
impl<T> Default for SnapshotTable<T> {
    fn default() -> Self {
        Self {
            pages: SharedVec::default(),
            len: 0,
        }
    }
}
impl<T> SnapshotTable<T> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }
    pub fn get(&self, slot: usize) -> Option<&T> {
        if slot >= self.len {
            return None;
        }
        Some(&self.pages[slot / PAGE_SIZE][slot % PAGE_SIZE])
    }
    pub fn iter(&self) -> SnapshotIter<'_, T> {
        SnapshotIter {
            table: self,
            range: 0..self.len,
        }
    }
    pub(super) fn push(&mut self, value: T) {
        if self.len % PAGE_SIZE == 0 {
            self.pages.push(SharedVec::default());
        }
        self.pages.last_mut().unwrap().push(Arc::new(value));
        self.len += 1;
    }
}
impl<T: Clone> SnapshotTable<T> {
    pub(super) fn last_mut(&mut self) -> Option<&mut T> {
        self.len.checked_sub(1).map(|slot| &mut self[slot])
    }
}
impl<T> Index<usize> for SnapshotTable<T> {
    type Output = T;
    fn index(&self, slot: usize) -> &T {
        self.get(slot).expect("object index slot")
    }
}
impl<T: Clone> IndexMut<usize> for SnapshotTable<T> {
    fn index_mut(&mut self, slot: usize) -> &mut T {
        assert!(slot < self.len, "object index slot");
        let record = &mut self.pages[slot / PAGE_SIZE][slot % PAGE_SIZE];
        #[cfg(feature = "bench-instrumentation")]
        let _span = kasane_core::benchmark::span(if Arc::strong_count(record) > 1 {
            "editor.index_record_cow_clone"
        } else {
            "editor.index_record_unique"
        });
        Arc::make_mut(record)
    }
}

pub struct SnapshotIter<'a, T> {
    table: &'a SnapshotTable<T>,
    range: Range<usize>,
}
impl<'a, T> Iterator for SnapshotIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.range.next().map(|i| &self.table[i])
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}
impl<T> DoubleEndedIterator for SnapshotIter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.range.next_back().map(|i| &self.table[i])
    }
}
impl<T> ExactSizeIterator for SnapshotIter<'_, T> {}
impl<'a, T> IntoIterator for &'a SnapshotTable<T> {
    type Item = &'a T;
    type IntoIter = SnapshotIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[derive(Debug)]
    struct Record {
        value: usize,
        clones: Rc<Cell<usize>>,
    }
    impl Clone for Record {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                value: self.value,
                clones: self.clones.clone(),
            }
        }
    }
    #[test]
    fn retained_readers_copy_only_modified_records_across_pages() {
        let clones = Rc::new(Cell::new(0));
        let mut original = SnapshotTable::default();
        for value in 0..100 {
            original.push(Record {
                value,
                clones: clones.clone(),
            });
        }
        let mut candidate = original.clone();
        assert_eq!(clones.get(), 0);
        candidate[33].value = 999;
        candidate[33].value = 777;
        candidate[99].value = 555;
        assert_eq!(clones.get(), 2);
        assert_eq!(original[33].value, 33);
        assert_eq!(original[99].value, 99);
        assert!(std::ptr::eq(&original[0], &candidate[0]));
        assert!(std::ptr::eq(&original[34], &candidate[34]));
        assert_eq!(candidate.iter().count(), 100);
        assert_eq!(candidate.iter().next_back().unwrap().value, 555);
    }
}
