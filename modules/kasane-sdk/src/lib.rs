//! Engine-independent Rust authoring SDK with project open/save and checkpoint history.
mod alpha_mesh;
mod assets;
mod diagnostics;
mod edit;
mod project_io;
mod session;
mod spatial;
mod types;
pub use kasane_animation::canonical_motion_seek_time;
pub use kasane_animation::CompiledSegment;

pub use alpha_mesh::{alpha_mesh_geometry, AlphaMask, AlphaMeshOptions};
pub use assets::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
};
pub use diagnostics::{GeometryBounds, GeometryChecks, GeometryDiagnostic, GeometryDiagnosticKind};
pub use edit::EditSession;
pub use kasane_animation::{
    apply_model_opacity, AnimationError, ExpressionPreview, ExpressionTransition, MotionEventFired,
    MotionPreview, MotionSnapshot, PhysicsOutputSnapshot, PhysicsPreview, PhysicsRigSnapshot,
    PreviewGeometryTimings, RuntimeSnapshot, SeekCacheStats,
};
pub use kasane_animation::{
    motion_bezier_time_is_monotonic, sample_motion_curve, CompiledCurve, MotionEditingPreview,
    MotionPlaybackPosition,
};
pub use kasane_project::{
    Can3Artifact, Can3Options, Cmo3Artifact, Cmo3Diagnostic, Cmo3Error, Cmo3Options, Cmo3Report,
    CubismArtifact, CubismOptions, FileSystem, ImportReport, LossPolicy, NativeFileSystem,
    ProjectResult, PsdImportReport, ResourceDiagnostic,
};
pub use session::{AuthoringSession, AuthoringSnapshot};
pub use spatial::{
    hit_test_geometry, object_bounds, resolve_mesh_ids, GeometryHit, GeometryQuery, ObjectBounds,
    ObjectTarget, SpatialError, TriangleHit,
};
pub use types::{
    EditReceipt, GeometrySnapshot, GeometrySpace, HistoryLimits, HistoryState, ImportReceipt,
    MeshProperties, ObjectHandle, ObjectKind, PsdImportReceipt, SaveReceipt, SdkError,
    TopologyReplacement, Version,
};

mod geometry;
pub use geometry::{rectangle_grid_geometry, MeshGeometry};

mod motion_edit;
pub use motion_edit::{
    MotionBezierHandle, MotionClipCandidate, MotionInterpolation, MotionKeyMove, MotionKeyRef,
};

mod expression_edit;
pub use expression_edit::{
    CapabilityState, ExpressionCapabilities, ExpressionCollection, ExpressionEditPlan,
    ExpressionEntryAddress, ExpressionEntryCandidate, ExpressionExportPlan,
};
pub use kasane_core::document::{ExpressionEdit, ExpressionEntryFields};

mod changes;
pub use changes::{ChangeCategory, ChangeSet, CollectionRef, EntityRef, ReferenceChange};
