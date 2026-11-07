#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// Select a Physics rig collection or rows within one rig.
pub enum PhysicsCollection {
    /// Select Physics rigs.
    Groups,
    /// Select inputs of one rig.
    Inputs {
        /// Stable Physics rig ID owning the selected rows.
        group_id: String,
    },
    /// Select outputs of one rig.
    Outputs {
        /// Stable Physics rig ID owning the selected rows.
        group_id: String,
    },
    /// Select nodes of one rig.
    Nodes {
        /// Stable Physics rig ID owning the selected rows.
        group_id: String,
    },
}
