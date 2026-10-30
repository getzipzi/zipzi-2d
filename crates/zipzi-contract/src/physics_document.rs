//! Persistent Physics authoring values.
use crate::physics as wire;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PhysicsTarget {
    Unspecified,
    Resolved {
        /// Stable document parameter UUID; evaluation uses its declared range and default.
        parameter_id: String,
    },
    Unresolved {
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
}

impl PhysicsTarget {
    pub fn parameter_id(&self) -> Option<&str> {
        if let Self::Resolved { parameter_id } = self {
            Some(parameter_id)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsNode {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Authored two-dimensional Physics node position or normalization domain, as declared by the field.
    pub position: wire::PhysicsVector,
    /// Native Physics node mobility coefficient.
    pub mobility: f32,
    /// Native Physics node delay coefficient.
    pub delay: f32,
    /// Native Physics node acceleration coefficient.
    pub acceleration: f32,
    /// Native Physics node segment length/radius.
    pub radius: f32,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, serde_json::Value>,
}

use crate::physics::{PhysicsAxis, PhysicsRange};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Atomic asset edits plus optional rig edits; asset-only patches support empty assets.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsPatch {
    /// Stable Physics singleton ID owning these changes.
    pub asset_id: String,
    /// Ordered changes to Physics asset metadata; rig-local changes are separate.
    pub asset_changes: Vec<PhysicsAssetProperty>,
    /// Optional atomic changes to one owned Physics rig/group.
    pub rig: Option<PhysicsRigPatch>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum PhysicsAssetProperty {
    /// None or zero follows the evaluation delta; positive values select a fixed step.
    Fps(
        /// Optional Physics fixed-step frequency; None follows the evaluation delta.
        Option<f32>,
    ),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct PhysicsRigPatch {
    /// Stable ID of the group owning the affected rows or node chain.
    pub group_id: String,
    /// Ordered typed property changes validated as one native operation.
    pub changes: Vec<PhysicsProperty>,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum PhysicsProperty {
    Name(
        /// Replacement display name of the Physics group.
        String,
    ),
    InputWeight {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    InputAxis {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: PhysicsAxis,
    },
    InputReflect {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: bool,
    },
    InputParameter {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: PhysicsTarget,
    },
    OutputWeight {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    OutputScale {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    OutputAxis {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: PhysicsAxis,
    },
    OutputReflect {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: bool,
    },
    OutputParameter {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: PhysicsTarget,
    },
    OutputNode {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Stable node identity, independent of the node index used by exported Physics JSON.
        node_id: String,
    },
    VertexPosition {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: crate::physics::PhysicsVector,
    },
    VertexRadius {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    VertexMobility {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    VertexDelay {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    VertexAcceleration {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Channel value or replacement payload in the domain declared by this field type.
        value: f32,
    },
    PositionNormalization(
        /// Replacement minimum/default/maximum range for position normalization.
        PhysicsRange,
    ),
    AngleNormalization(
        /// Replacement minimum/default/maximum range for angle normalization.
        PhysicsRange,
    ),
}
impl PhysicsProperty {
    pub fn entity_id(&self) -> Option<&str> {
        use PhysicsProperty::*;
        match self {
            InputWeight { id, .. }
            | InputAxis { id, .. }
            | InputReflect { id, .. }
            | InputParameter { id, .. }
            | OutputWeight { id, .. }
            | OutputScale { id, .. }
            | OutputAxis { id, .. }
            | OutputReflect { id, .. }
            | OutputParameter { id, .. }
            | OutputNode { id, .. }
            | VertexPosition { id, .. }
            | VertexRadius { id, .. }
            | VertexMobility { id, .. }
            | VertexDelay { id, .. }
            | VertexAcceleration { id, .. } => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PhysicsEdit {
    CreateGroup {
        /// Stable Physics singleton ID owning these changes.
        asset_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    CreateInput {
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    CreateOutput {
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Stable node identity, independent of the node index used by exported Physics JSON.
        node_id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    InsertNode {
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
        /// Complete Physics node to insert, including its stable identity and authored dynamics.
        node: PhysicsNode,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    MoveNode {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DuplicateNode {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    Patch {
        /// Typed atomic Physics changes validated against the current candidate.
        patch: PhysicsPatch,
    },
    MoveInputs {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    MoveOutputs {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
        /// Explicit old-node to replacement-node identities used when repairing output references.
        node_replacements: BTreeMap<String, String>,
    },
    ReorderGroups {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    Duplicate {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: Option<String>,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    Delete {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit old-node to replacement-node identities used when repairing output references.
        node_replacements: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsInput {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
    pub source: PhysicsTarget,
    /// Owned opaque fields attached to an imported Physics parameter reference.
    pub reference_extensions: BTreeMap<String, serde_json::Value>,
    /// Native Physics influence weight, expressed as a percentage.
    pub weight: f32,
    /// Native Physics axis/channel discriminator; distinct from an object identity.
    pub kind: wire::PhysicsAxis,
    /// Whether the native Physics mapping reflects the selected axis.
    pub reflect: bool,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsOutput {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Explicit destination owner/container; order is controlled separately by before.
    pub destination: PhysicsTarget,
    /// Owned opaque fields attached to an imported Physics parameter reference.
    pub reference_extensions: BTreeMap<String, serde_json::Value>,
    /// Stable node identity, independent of the node index used by exported Physics JSON.
    pub node_id: String,
    /// Native Physics output multiplier applied to the selected axis.
    pub scale: f32,
    /// Native Physics influence weight, expressed as a percentage.
    pub weight: f32,
    /// Native Physics axis/channel discriminator; distinct from an object identity.
    pub kind: wire::PhysicsAxis,
    /// Whether the native Physics mapping reflects the selected axis.
    pub reflect: bool,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsSetting {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Imported/exported Physics rig label, distinct from its persistent group UUID.
    pub external_id: String,
    /// Ordered Physics input rows, each with a stable identity and typed parameter source.
    pub inputs: Vec<PhysicsInput>,
    /// Ordered Physics output rows, each with a stable identity and node reference.
    pub outputs: Vec<PhysicsOutput>,
    /// Ordered Physics node chain; outputs refer to stable node IDs.
    pub vertices: Vec<PhysicsNode>,
    /// Native position and angle normalization ranges for this Physics rig.
    pub normalization: wire::PhysicsNormalization,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsDefinition {
    /// Native Physics format version number.
    pub version: u32,
    /// Physics format metadata and counts maintained by native authoring validation.
    pub meta: wire::PhysicsMeta,
    /// Ordered shared Physics rig definitions.
    pub settings: Vec<Arc<PhysicsSetting>>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsAsset {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Complete persistent Physics definition; simulation state lives outside the document.
    pub data: PhysicsDefinition,
    /// Original runtime-ID namespace used to validate preservation of opaque imported fields.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Original imported-content hash used to detect unsupported changes to opaque fields.
    pub opaque_source_content_hash: Option<String>,
}

impl PhysicsSetting {
    pub fn entity_ids(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str())
            .chain(self.inputs.iter().map(|v| v.id.as_str()))
            .chain(self.outputs.iter().map(|v| v.id.as_str()))
            .chain(self.vertices.iter().map(|v| v.id.as_str()))
    }
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.vertices.iter().position(|v| v.id == id)
    }
}
impl PhysicsDefinition {
    pub fn actual_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.settings.len(),
            self.settings.iter().map(|g| g.inputs.len()).sum(),
            self.settings.iter().map(|g| g.outputs.len()).sum(),
            self.settings.iter().map(|g| g.vertices.len()).sum(),
        )
    }
    pub fn counts_match(&self) -> bool {
        self.actual_counts()
            == (
                self.meta.setting_count,
                self.meta.input_count,
                self.meta.output_count,
                self.meta.vertex_count,
            )
    }
}
impl PhysicsAsset {
    pub fn entity_ids(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str())
            .chain(self.data.settings.iter().flat_map(|g| g.entity_ids()))
    }

    pub fn reconcile_counts(&mut self) {
        let (g, i, o, n) = self.data.actual_counts();
        self.data.meta.setting_count = g;
        self.data.meta.input_count = i;
        self.data.meta.output_count = o;
        self.data.meta.vertex_count = n;
    }
    pub fn group_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.data
            .meta
            .dictionary
            .iter()
            .find(|d| d.id == id)
            .map_or(id, |d| d.name.as_str())
    }
}
impl PhysicsAsset {
    pub fn empty(id: String) -> Self {
        Self {
            id,
            data: PhysicsDefinition {
                version: 3,
                meta: crate::physics::PhysicsMeta {
                    setting_count: 0,
                    input_count: 0,
                    output_count: 0,
                    vertex_count: 0,
                    fps: Some(30.),
                    forces: crate::physics::PhysicsForces {
                        gravity: crate::physics::PhysicsVector { x: 0., y: -1. },
                        wind: crate::physics::PhysicsVector { x: 0., y: 0. },
                        extensions: BTreeMap::new(),
                    },
                    dictionary: vec![],
                    extensions: BTreeMap::new(),
                },
                settings: vec![],
                extensions: BTreeMap::new(),
            },
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        }
    }
}
