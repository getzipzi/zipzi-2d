//! Bezier controller values and local validation.
use crate::model::{Status, Vec2};
use serde::{Deserialize, Serialize};

/// Editor influence semantics, independent of runtime Warp interpolation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(rename_all = "snake_case")]
pub enum WarpBezierEditMode {
    #[default]
    /// Edit explicit controls; anchors carry their own handles.
    Local,
    /// Cosine influence on nearby nodes within two controller cells.
    SmoothLocal,
    /// Gaussian influence across normalized controller coordinates.
    SmoothGlobal,
    /// Couple opposite handles while preserving their length ratio; anchors stay fixed.
    PreserveStructure,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Controller subdivision density, independent of baked Warp lattice density.
pub struct WarpBezierGrid {
    /// Vertical controller cell count, from 1 to 32.
    pub rows: u32,
    /// Horizontal controller cell count, from 1 to 32.
    pub columns: u32,
}
impl WarpBezierGrid {
    pub fn validate(self) -> Status {
        if self.rows == 0 || self.columns == 0 || self.rows > 32 || self.columns > 32 {
            Status::error(
                "INVALID_WARP_BEZIER",
                "Controller subdivisions must be from 1 to 32",
            )
        } else {
            Status::ok()
        }
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Persistent editing mode, reform policy and optional controller density for a Warp.
pub struct WarpAuthoringSettings {
    /// Rebuild the edited shape's baked grid on subsequent Bezier edits.
    #[serde(default)]
    pub auto_reform: bool,
    /// Neighborhood or tangent-coupling rule for subsequent control edits.
    #[serde(default)]
    pub edit_mode: WarpBezierEditMode,
    /// Explicit controller density shared by stored shapes; None retains imported or derived density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezier_grid: Option<WarpBezierGrid>,
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
/// Absolute destination for one controller anchor or handle.
pub struct WarpBezierControlPatch {
    /// Zero-based controller node in row-major order.
    pub node: u32,
    /// 0 is the anchor; 1..4 are left, right, up and down handles.
    pub component: u8,
    /// Absolute destination in the Warp's parent authoring space.
    pub point: Vec2,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(rename_all = "snake_case")]
/// Choose how to synchronize a stored Warp controller and baked shape.
pub enum WarpShapeAction {
    /// Refit a controller from the existing baked lattice, preserving the baked points.
    ResetController,
    /// Replace baked points by sampling the controller.
    Reform,
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
/// One controller anchor with its four handles.
pub struct WarpBezierNode {
    /// Absolute Bezier anchor position in lattice coordinates; not a handle offset.
    pub position: Vec2,
    /// Absolute positions: left, right, up, down in lattice coordinates.
    pub handles: [Vec2; 4],
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
/// Row-major Bezier controller for an authored Warp shape.
pub struct WarpBezier {
    /// Number of warp lattice cells vertically; a lattice has rows + 1 point rows.
    pub rows: u32,
    /// Number of warp lattice cells horizontally; a lattice has columns + 1 point columns.
    pub columns: u32,
    /// Bezier nodes in row-major order; exactly (rows + 1) * (columns + 1) nodes.
    pub nodes: Vec<WarpBezierNode>,
}
impl WarpBezier {
    pub fn validate(&self) -> Status {
        if self.rows == 0
            || self.columns == 0
            || self.rows > 32
            || self.columns > 32
            || self.nodes.len() != ((self.rows + 1) * (self.columns + 1)) as usize
        {
            return Status::error("INVALID_WARP_BEZIER", "Invalid controller dimensions");
        }
        if self
            .nodes
            .iter()
            .flat_map(|n| std::iter::once(&n.position).chain(n.handles.iter()))
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Status::error("INVALID_WARP_BEZIER", "Non-finite controller position");
        }
        Status::ok()
    }
    pub fn has_handle(&self, node: usize, handle: usize) -> bool {
        let x = node % (self.columns as usize + 1);
        let y = node / (self.columns as usize + 1);
        match handle {
            0 => x > 0,
            1 => x < self.columns as usize,
            2 => y > 0,
            3 => y < self.rows as usize,
            _ => false,
        }
    }
}
