//! Engine-independent Rust authoring SDK with project open/save and checkpoint history.
mod alpha_mesh;
mod assets;
mod diagnostics;
mod edit;
mod project_io;
mod session;
mod spatial;
mod types;
pub use zipzi_animation::canonical_motion_seek_time;
pub use zipzi_animation::CompiledSegment;

pub use alpha_mesh::{alpha_mesh_geometry, AlphaMask, AlphaMeshOptions};
pub use assets::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
};
pub use diagnostics::{GeometryBounds, GeometryChecks, GeometryDiagnostic, GeometryDiagnosticKind};
pub use edit::EditSession;
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
pub use zipzi_animation::{
    apply_model_opacity, AnimationError, ExpressionPreview, ExpressionTransition, MotionEventFired,
    MotionPreview, MotionSnapshot, PhysicsOutputSnapshot, PhysicsPreview, PhysicsRigSnapshot,
    PreviewGeometryTimings, RuntimeSnapshot, SeekCacheStats,
};
pub use zipzi_animation::{
    motion_bezier_time_is_monotonic, sample_motion_curve, CompiledCurve, MotionEditingPreview,
    MotionPlaybackPosition,
};
pub use zipzi_project::{
    Can3Artifact, Can3Options, Cmo3Artifact, Cmo3Diagnostic, Cmo3Error, Cmo3Options, Cmo3Report,
    CubismArtifact, CubismOptions, FileSystem, ImportReport, LossPolicy, NativeFileSystem,
    ProjectResult, PsdImportReport, ResourceDiagnostic,
};

mod geometry;
pub use geometry::{rectangle_grid_geometry, MeshGeometry};
mod mesh_binding_edit;
mod mesh_edit;
mod scene_edit;
pub use mesh_binding_edit::{
    MeshBindingAxisSpec, MeshBindingCreateSpec, MeshBindingOperation, MeshKeyformSpec,
};
pub use mesh_edit::{MeshCreateSpec, MeshOperation};
pub use scene_edit::{
    GluePairSpec, OffscreenFormSpec, ParameterGroupCreateSpec, PartCreateSpec, ScalarSample,
    SceneOperation, ScenePlacement, SceneSummary, VertexPatchSpec, VertexWeightSpec,
};
mod parameter_edit;
pub use parameter_edit::{ParameterCreateSpec, ParameterOperation, ParameterPatch};
mod image_edit;
pub use image_edit::ImageCreateSpec;

mod motion_edit;
pub use motion_edit::{
    MotionBezierHandle, MotionClipCandidate, MotionInterpolation, MotionKeyMove, MotionKeyRef,
};

mod expression_edit;
pub use expression_edit::{
    CapabilityState, ExpressionCapabilities, ExpressionCollection, ExpressionEditPlan,
    ExpressionEntryAddress, ExpressionEntryCandidate, ExpressionExportPlan,
};
pub use zipzi_core::document::{ExpressionEdit, ExpressionEntryFields};

mod changes;
pub use changes::PoseReferenceChange;
pub use changes::{ChangeCategory, ChangeSet, CollectionRef, EntityRef, ReferenceChange};

mod motion_authoring;
pub use motion_authoring::*;

pub use changes::BindingReferenceChange;
pub use changes::{MotionReferenceChange, MotionReferenceValue};

mod physics_authoring;
pub use physics_authoring::*;
mod binding_authoring;
pub use binding_authoring::*;

mod pose_authoring;
pub use pose_authoring::*;

mod organization_authoring;
pub use organization_authoring::*;

pub use changes::OrganizationReferenceChange;

mod scene_authoring;
pub use changes::{
    SceneContentChange, SceneContentReferenceChange, SceneContentReferenceValue, SceneFieldChange,
    SceneReferenceChange,
};
pub use scene_authoring::*;

mod scene_content;
pub use scene_content::*;

mod reference_authoring;
pub use reference_authoring::*;

pub use changes::{ReferenceFieldChange, ReferenceSelectionChange};

mod resource_authoring;
pub use resource_authoring::*;

mod document_authoring;
pub use document_authoring::*;

mod authoring_collections;
pub use authoring_collections::*;

mod modeling;
pub use modeling::{
    modeling_records, ModelingIntent, ModelingPlan, ModelingPlanReport, ModelingQuery,
    ModelingRecords, ModelingSamples, OwnedModelingRecords,
};
