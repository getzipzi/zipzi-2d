mod atlas;
mod export_preflight;
pub use export_preflight::*;
pub mod cdi;
pub mod cmo3;
pub mod codec;
pub mod cubism;
pub mod expression;
pub mod filesystem;
pub mod model3;
pub mod motion;
pub mod package;
pub mod physics;
pub mod pose;
pub mod resource_budget;
pub mod resources;
pub use resource_budget::{
    ImageBufferKind, ImageMemoryBudget, ImageMemoryReservation, ImageMemoryUsage,
    TransferReservation,
};
pub mod store;
pub mod transfer;

pub use cdi::{export_cdi3, import_cdi3, CdiImport, CdiProjectDiagnostic, CdiProjectError};
pub use cmo3::{encode_document_cmo3, encode_project_cmo3, publish_cmo3};
pub use codec::{decode_project, encode_project};
#[cfg(feature = "binary-prototype")]
pub use codec::{decode_project_cbor, encode_project_cbor};
pub use cubism::{encode_document_cubism, encode_project_cubism, publish_can3, publish_cubism};
pub use expression::{
    export_available_expression3, export_expression3, import_expression3, ExpressionDiagnostic,
    ExpressionImport, ExpressionProjectError,
};
pub use filesystem::{FileSystem, NativeFileSystem, Publication};
pub use motion::{
    export_available_motion3, export_motion3, import_motion3, MotionDiagnostic, MotionImport,
    MotionProjectError,
};
pub use package::{
    build_export_plan, publish_export_plan, publish_package, ExportFile, ExportFileKind,
    ExportPlan, PackageOptions, PackageReference, PackageValidation, PackageValidator,
    RuntimeValidation,
};
pub use physics::{
    export_available_physics3, export_physics3, import_physics3, PhysicsDiagnostic, PhysicsImport,
    PhysicsProjectError,
};
pub use pose::{
    export_available_pose3, export_pose3, import_pose3, PoseDiagnostic, PoseImport,
    PoseProjectError,
};
pub use resources::{content_sha256, decode_png, is_valid_asset_path, AssetData, TextureData};
pub use store::{
    prepare_image_content, prepare_image_content_with_budget, prepare_png_content,
    prepare_project_image, project_identity, project_manifest, read_project_asset,
    read_project_texture_bounded, DocumentSession, DocumentSnapshot, DocumentStore,
    ImageResourceStore, PreparedImageContent, ProjectResult, ResourceDiagnostic, VerifiedTexture,
};
pub use transfer::{resolve_model3_entry, TransferFormat, TransferSnapshot};
pub use zipzi_cmo3::{
    Can3Artifact, Can3Options, Cmo3Artifact, Cmo3Diagnostic, Cmo3Error, Cmo3Options, Cmo3Report,
    CubismArtifact, CubismOptions, DiagnosticSeverity as Cmo3DiagnosticSeverity, LossPolicy,
};
pub use zipzi_moc3::ImportReport;
pub use zipzi_psd::ImportReport as PsdImportReport;

/// Publish an already encoded sidecar with the same staging/durability contract
/// as editable Cubism files. The caller owns encoding and its typed report.
pub fn publish_export_bytes(
    bytes: &[u8],
    destination: &std::path::Path,
) -> Result<Publication, Cmo3Error> {
    cmo3::publish_bytes(bytes, destination, &filesystem::NativeFileSystem)
}
