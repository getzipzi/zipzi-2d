//! Native configurable scene targets. Empty and imported unresolved references
//! retain their meaning instead of being replaced by invented object IDs.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneReference {
    #[default]
    Unspecified,
    Resolved {
        /// Stable document ID of the object described or selected by this value.
        object_id: String,
    },
    Unresolved {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
    },
}
impl SceneReference {
    pub fn resolved_id(&self) -> Option<&str> {
        match self {
            Self::Resolved { object_id } => Some(object_id),
            _ => None,
        }
    }
    /// Compatibility view for format adapters that separately validate readiness.
    pub fn as_str(&self) -> &str {
        self.resolved_id().unwrap_or("")
    }
    pub fn capacity(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Resolved { object_id } => object_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
    pub fn validate(&self) -> crate::Status {
        match self {
            Self::Resolved { object_id }
                if uuid::Uuid::parse_str(object_id).is_err()
                    || uuid::Uuid::parse_str(object_id)
                        .is_ok_and(|id| id.to_string() != *object_id) =>
            {
                crate::Status::error("INVALID_REFERENCE", object_id)
            }
            Self::Unresolved { runtime_id } if runtime_id.is_empty() => crate::Status::error(
                "INVALID_REFERENCE",
                "Unresolved reference needs a runtime ID",
            ),
            _ => crate::Status::ok(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }
    pub fn remap(&mut self, ids: &std::collections::BTreeMap<String, String>) {
        if let Self::Resolved { object_id } = self {
            if let Some(new) = ids.get(object_id) {
                *object_id = new.clone();
            }
        }
    }
}
impl From<String> for SceneReference {
    fn from(object_id: String) -> Self {
        if object_id.is_empty() {
            Self::Unspecified
        } else {
            Self::Resolved { object_id }
        }
    }
}
impl From<&str> for SceneReference {
    fn from(id: &str) -> Self {
        id.to_owned().into()
    }
}
impl std::fmt::Display for SceneReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unspecified => f.write_str("unspecified"),
            Self::Resolved { object_id } => f.write_str(object_id),
            Self::Unresolved { runtime_id } => write!(f, "unresolved: {runtime_id}"),
        }
    }
}
impl std::ops::Deref for SceneReference {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl PartialEq<String> for SceneReference {
    fn eq(&self, other: &String) -> bool {
        self.resolved_id() == Some(other.as_str())
    }
}
impl PartialEq<&str> for SceneReference {
    fn eq(&self, other: &&str) -> bool {
        self.resolved_id() == Some(*other)
    }
}
