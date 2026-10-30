//! Stable identity for editable rows.
pub use zipzi_contract::document::ReferenceEntry;
pub trait ReferenceEntryAllocation<T> {
    fn new(target: T) -> Self;
}
impl<T> ReferenceEntryAllocation<T> for ReferenceEntry<T> {
    fn new(target: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            target,
        }
    }
}
