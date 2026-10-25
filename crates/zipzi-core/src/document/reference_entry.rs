//! Stable identity for editable rows; the target remains a shared reference.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ReferenceEntry<T> {
    pub id: String,
    pub target: T,
}
impl<T> ReferenceEntry<T> {
    pub fn new(target: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            target,
        }
    }
}
impl<T> From<T> for ReferenceEntry<T> {
    fn from(target: T) -> Self {
        Self::new(target)
    }
}
