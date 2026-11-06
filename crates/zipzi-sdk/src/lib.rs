//! Immutable authoring snapshots, isolated Drafts, publication and native project I/O.
//! Domain algorithms use Builder; host/client consumers migrate in the following stage;
//! the old EditSession/DocumentSession execution path is not exported here.
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
    ManualMeshOutcome, ManualMeshTransferReport, ManualMeshUvMap, VertexProvenance,
};

mod motion_curve;
pub use motion_curve::{
    MotionBezierHandle, MotionCurveEdit, MotionInterpolation, MotionKeyMove, MotionKeyRef,
};
