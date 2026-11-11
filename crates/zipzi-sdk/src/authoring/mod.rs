//! Snapshot authoring, native project I/O and sole formal publication ownership.
mod commands;
mod draft;
mod history;
mod project;
mod session;
mod snapshot;
pub use project::{
    CompletedSave, OpenReceipt, PreparedProject, PreparedSave, ProjectContext, SaveReceipt,
    SavedMarker,
};

pub use commands::{Commands, NameTarget};
pub use draft::{Draft, PreparedChange, PreparedPreview};
pub use history::{HistoryLimits, HistoryState};
pub use session::{AuthoringSession, Publication, PublicationKind};
pub use snapshot::{Snapshot, SourcedSnapshot};
pub use zipzi_contract::authoring::{
    MeshBindingAxisSpec, MeshBindingCreateSpec, MeshKeyformSpec, ParameterCreateSpec,
    ParameterPatch,
};
pub use zipzi_contract::document::AttachmentEdit;
pub use zipzi_contract::document::{ExpressionEdit, ExpressionEntryFields};
pub use zipzi_contract::document::{MotionContainer, MotionEdit};
pub use zipzi_contract::document::{PoseCollection, PoseEdit, PoseFields};
pub use zipzi_contract::kernel::{
    ContentId, DraftId, DraftSource, QuerySource, SessionKey, Version,
};
pub use zipzi_contract::physics_document::{
    PhysicsAssetProperty, PhysicsEdit, PhysicsPatch, PhysicsProperty, PhysicsRigPatch,
};

use zipzi_core::Status;
fn error(code: &str, message: &str) -> Status {
    Status::error(code, message)
}

#[cfg(test)]
mod tests;

pub use zipzi_contract::document::{
    AxisReduction, BindingGridEdit, BindingGridTarget, BlendAuthoringEdit, BlendTableOperation,
    GridOperation, ParameterRangeEdit, ParameterRangePolicy,
};
pub use zipzi_core::document::{ConversionAddress, ConversionElement, ConversionReport};
pub use zipzi_core::state::{
    ConversionControl, ConversionLimits, ConversionOutcome, ConversionWork,
};

pub use zipzi_contract::document::{
    OrganizationCollection, OrganizationDeletePolicy, OrganizationEdit,
};
pub use zipzi_core::state::OrganizationOutcome;

pub use zipzi_contract::document::{
    ReparentContext, ReparentEdit, ReparentPlacement, ReparentStrategy, SceneCollection,
    SceneCopyScope, SceneCreate, SceneEdit, SceneField, SceneFieldValue,
};
pub use zipzi_core::state::SceneOutcome;

pub use zipzi_contract::document::{
    NewMeshVertex, OffscreenDeletePolicy, OffscreenMappingPatch, PartKeyformSlot, SceneContentEdit,
    SceneContentReport, SceneContentTarget, SceneContentValue, ShapeTarget, VertexContentPatch,
    VertexDeletePolicy, VertexSeed, VertexWeight, WarpPointPatch,
};
pub use zipzi_core::state::SceneContentOutcome;

pub use zipzi_contract::document::{
    ReferenceCollection, ReferenceEdit, ReferenceFields, ReferenceSelectionField,
};
pub use zipzi_core::state::ReferenceOutcome;

pub use zipzi_contract::authoring::{GeometrySpace, MeshCreateSpec};

pub use zipzi_contract::kernel_commands::{Command, CommandOutcome, ImageInput, ObjectAddress};
