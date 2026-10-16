use serde::{Deserialize, Serialize};

/// Layout selection. Editing contexts and viewport identities are separate state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    #[default]
    Modeling,
    Animation,
}

impl WorkspaceKind {
    pub const ALL: [Self; 2] = [Self::Modeling, Self::Animation];

    pub const fn index(self) -> usize {
        match self {
            Self::Modeling => 0,
            Self::Animation => 1,
        }
    }
}
