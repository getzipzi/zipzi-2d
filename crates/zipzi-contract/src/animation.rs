//! Animation observation values.
use std::collections::BTreeMap;

/// Read-only observation of one rig. Positions are the latest solved physics
/// step, not model-space attachment points or interpolated render geometry.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct PhysicsRigSnapshot {
    /// Native physics group UUID.
    pub id: String,
    /// Input channel UUIDs aligned with inputs.
    pub input_ids: Vec<String>,
    /// Output channel UUIDs aligned with outputs.
    pub output_ids: Vec<String>,
    /// Particle node UUIDs aligned with particles.
    pub node_ids: Vec<String>,
    /// Latest solved particle XY positions in the SDK solver coordinate system.
    pub particles: Vec<[f32; 2]>,
    /// Weighted, normalized angle in degrees used by the latest solved step.
    /// Rotate the Y-down solver coordinates by this angle to display a fixed
    /// gravity direction with rotating input axes.
    pub input_angle: f32,
    /// Latest normalized input values in channel order.
    pub inputs: Vec<f32>,
    /// Latest output observations; None marks an unavailable channel.
    pub outputs: Vec<Option<PhysicsOutputSnapshot>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct PhysicsOutputSnapshot {
    /// Raw output value before weight and parameter clamping.
    pub raw: f32,
    /// Parameter value immediately before this output.
    pub before: f32,
    /// Parameter value after applying this output and clamping.
    pub after: f32,
    /// Whether the SDK clamped this output to the target parameter range.
    pub clamped: bool,
}

/// A motion event crossed during the last update, returned without side effects.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct MotionEventFired {
    /// Source clip UUID.
    pub motion_id: String,
    /// Source event UUID.
    pub event_id: String,
    /// User-authored event text.
    pub value: String,
}

/// Combined values after Motion, Expression, Physics and Pose evaluation.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct MotionSnapshot {
    /// Optional inspector observations. Collected explicitly by a host after
    /// evaluation. Advance/seek results leave this empty; populate it with
    /// `snapshot.physics = preview.physics_snapshot();` when displaying an inspector.
    /// Internal observations are retained in replay checkpoints for consistent seeks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub physics: Vec<PhysicsRigSnapshot>,
    /// Absolute preview time in seconds.
    pub time: f32,
    /// Final parameter values keyed by parameter UUID.
    pub parameters: BTreeMap<String, f32>,
    /// PartOpacity curves write virtual controls when no real parameter has the Part runtime ID.
    pub part_opacity_channels: BTreeMap<String, f32>,
    /// Pose-produced opacities keyed by Part UUID.
    pub part_opacities: BTreeMap<String, f32>,
    /// Model Opacity channel, separate from the drawable geometry opacity.
    pub model_opacity: f32,
    /// Active clip UUIDs in queue order; repeated activations may repeat a UUID.
    pub active_motions: Vec<String>,
    /// Active expression UUIDs in queue order, including fading-out entries.
    pub active_expressions: Vec<String>,
    /// Events crossed by the last update, not accumulated across all seek steps.
    pub fired_events: Vec<MotionEventFired>,
    /// Model EyeBlink/LipSync mappings and unsupported Model IDs are reported.
    pub coverage: Vec<String>,
    /// SDK sampling coordinates, including the duration + 1 / FPS loop tail.
    pub motion_positions: Vec<MotionPlaybackPosition>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct MotionPlaybackPosition {
    /// Source motion clip UUID for this SDK playback position.
    pub motion_id: String,
    /// SDK local sample time in seconds, including the loop tail.
    pub local_time: f32,
    /// SDK loop period in seconds, including the FPS tail.
    pub period: f32,
    /// Canonical SDK loop index; exact seams belong to the preceding loop.
    pub loop_index: u64,
    /// Absolute preview-clock time at the start of this cycle.
    pub cycle_origin: f32,
}
