//! Values for the new authoring kernel. These are not the legacy wire protocol.
//! Session/Draft ownership and source authorization belong to SDK and Runtime.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! identity {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
        impl crate::api_shape::ApiShape for $name {
            fn api_shape() -> crate::api_shape::Shape {
                crate::api_shape::Shape::String
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

identity!(ClientId, "Identity of an authenticated Runtime client.");
identity!(JobId, "Identity of an owned Runtime task.");
identity!(BlobId, "Identity of client-owned immutable bulk bytes.");
identity!(RuntimeInstanceId, "Identity of a host process lifetime.");
identity!(
    ContentId,
    "Frozen content identity; history may restore the same identity."
);
identity!(
    SessionKey,
    "Identity of one authoring Session, including standalone Sessions."
);
identity!(
    DraftId,
    "Identity of one isolated candidate; not an authorization token."
);
identity!(
    ResourceKey,
    "Immutable resource version, independent of asset ID, path and encoding."
);

/// Publication identity. Restoring old content must still advance revision.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub session: SessionKey,
    pub generation: u64,
    pub revision: u64,
}

impl Version {
    /// Overflow rejects publication; it must never make an old source valid again.
    pub fn next_publication(self) -> Option<Self> {
        Some(Self {
            revision: self.revision.checked_add(1)?,
            ..self
        })
    }

    /// Document replacement advances both generation and publication revision.
    pub fn next_generation(self) -> Option<Self> {
        Some(Self {
            generation: self.generation.checked_add(1)?,
            ..self.next_publication()?
        })
    }
}

/// Exact candidate source for queries and prepared worker results.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(deny_unknown_fields)]
pub struct DraftSource {
    pub draft: DraftId,
    pub base: Version,
    pub sequence: u64,
}

impl DraftSource {
    pub fn next_sequence(self) -> Option<Self> {
        Some(Self {
            sequence: self.sequence.checked_add(1)?,
            ..self
        })
    }
}

/// Queries preserve whether they observed formal or candidate content.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(
    tag = "kind",
    content = "source",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum QuerySource {
    Published(Version),
    Draft(DraftSource),
}

/// Persistent image object. Storage locations never enter resolved model content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(deny_unknown_fields)]
pub struct AssetRecord {
    pub id: String,
    pub name: String,
    pub content: AssetContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetContent {
    Empty,
    Unresolved {
        source: String,
    },
    Image {
        resource: ResourceKey,
        width: u32,
        height: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_distinguish_session_generation_publication_and_candidate() {
        let base = Version {
            session: SessionKey::new(),
            generation: 1,
            revision: 1,
        };
        let draft = DraftSource {
            draft: DraftId::new(),
            base,
            sequence: 0,
        };
        assert_ne!(base, base.next_publication().unwrap());
        assert_ne!(
            base,
            Version {
                session: SessionKey::new(),
                ..base
            }
        );
        let replacement = base.next_generation().unwrap();
        assert_eq!((replacement.generation, replacement.revision), (2, 2));
        assert_ne!(draft, draft.next_sequence().unwrap());
        assert_ne!(
            draft,
            DraftSource {
                draft: DraftId::new(),
                ..draft
            }
        );
        assert_ne!(QuerySource::Published(base), QuerySource::Draft(draft));
        let source = QuerySource::Draft(draft);
        assert_eq!(
            serde_json::from_str::<QuerySource>(&serde_json::to_string(&source).unwrap()).unwrap(),
            source
        );
    }

    #[test]
    fn source_counters_never_wrap() {
        let base = Version {
            session: SessionKey::new(),
            generation: u64::MAX,
            revision: u64::MAX,
        };
        assert!(base.next_publication().is_none());
        assert!(base.next_generation().is_none());
        assert!(Version {
            revision: 0,
            ..base
        }
        .next_generation()
        .is_none());
        assert!(DraftSource {
            draft: DraftId::new(),
            base,
            sequence: u64::MAX
        }
        .next_sequence()
        .is_none());
    }
}
