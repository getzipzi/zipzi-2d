//! Bezier controller values and local validation.
use crate::model::{Status, Vec2};
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
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
