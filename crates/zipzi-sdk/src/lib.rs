//! Immutable authoring snapshots, isolated Drafts, publication and native project I/O.
//! Domain commands write through Builder. Published snapshots retain frozen model
//! state and the exact resource versions referenced by that state.
mod authoring;
pub use authoring::*;
pub use zipzi_core::Status;
pub use zipzi_project::{ProjectIo, ResourceStore, ResourceView};

mod manual_mesh;
pub use manual_mesh::*;
mod alpha_mesh;
pub use alpha_mesh::*;
mod mesh_quality;
pub use mesh_quality::*;

mod geometry;
pub use geometry::*;
mod manual_mesh_transfer;
pub use manual_mesh_transfer::{
    ManualMeshInspection, ManualMeshOutcome, ManualMeshTransferReport, ManualMeshUvMap,
    VertexProvenance,
};

mod motion_curve;
pub use motion_curve::{
    MotionBezierHandle, MotionCurveEdit, MotionInterpolation, MotionKeyMove, MotionKeyRef,
};

mod spatial;
pub use spatial::{
    hit_test_geometry, object_bounds, query_spatial_frame, validate_spatial_queries, GeometryHit,
    GeometryQuery, ObjectBounds, SpatialError, SpatialQuery, SpatialResult, TriangleHit,
};

#[cfg(test)]
#[path = "../tests/motion_editing.rs"]
mod motion_curve_regression;
