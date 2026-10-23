//! UI-independent editor coordination, SDK-backed document editing and CPU-only previews.
mod animation;
mod physics;
pub use physics::*;
mod animation_resources;
mod parameter_layout;
pub use parameter_layout::{ParameterLayout, ParameterRow};
mod timeline;
pub use animation_resources::*;
pub use timeline::*;
mod document;
mod transfer;
pub use animation::*;
pub use transfer::*;
mod editing_context;
mod inspection;
pub mod property;
pub use inspection::*;
pub use property::{
    PropertyEditToken, PropertyKey, PropertyPatch, PropertyScope, PropertySpec, PropertyType,
    PropertyValue, PropertyValues, property_spec, property_specs,
};
mod editor;
mod objects;
mod scene;
mod session;
pub use editing_context::EditingContext;
mod thumbnail;
mod tree;
pub use objects::*;
pub use thumbnail::{MESH_THUMBNAIL_SIZE, MeshTextureRegion};
mod workspace;
mod workspace_kind;
pub use workspace_kind::WorkspaceKind;

pub use document::{
    AlphaMeshOptions, AuthoringSessionReason, Document, ExpressionAuthoringCapabilities,
    ExpressionEntryEditToken, Layer, MeshGenerationOptions, MeshGenerationPreview,
    MeshGenerationSource, MeshGridOptions, NewParameter, ObjectState, ObjectStateIndex,
    OperationResult, OrganizationAuthoringCapabilities, PaletteMove, PaletteMoveDestination,
    PaletteMovePlan, PaletteMovePlanningError, PalettePosition, PalettePreservation,
    PaletteProjection, PaletteReparentCapture, PaletteReparentPlan, ParameterCreateError,
    ParameterDraft, ParameterExportDiagnostic, ParameterGroupDraft, ParameterGroupOption,
    ParameterGroupValue, ParameterKey, ParameterManagementError, ParameterValidationError,
    ParameterValue, ParameterVisual, PhysicsAuthoringCapabilities, PoseAuthoringCapabilities,
    ReferenceAuthoringCapabilities, ResourceAuthoringCapabilities, SceneAuthoringCapabilities,
    SceneContentAuthoringCapabilities, SceneObjectCreationPlan, SceneObjectType,
};
pub use document::{OrganizationDestination, ParameterDestination, ReorderPosition};
pub use editor::{CloseDisposition, EditorCommand, EditorPreview, LoadRequest};
pub use scene::{MAX_ZOOM, MIN_ZOOM, SceneSnapshot, SceneTexture, ViewState};
pub use tree::{TreeNode, TreeNodeKind};

use session::DocumentSession;
pub use workspace::{
    DocumentId, DocumentSummary, EditingContextToken, EditingLifecycle, Editor, LoadCompletion,
    LoadTask, OperationId, SaveCompletion, SaveTask,
};

mod panels;
pub use panels::*;

mod commands;
mod keyboard;
mod recent_files;
mod settings;
pub use commands::*;
pub use keyboard::*;
pub use recent_files::*;
pub use settings::*;

mod tools;
mod viewport;
pub use tools::*;
pub use viewport::*;

mod picking;
pub use picking::*;
mod canvas_topology;
pub use canvas_topology::{CanvasObject, CanvasObjectIndex};
mod control_edit;
mod deformer;
pub use deformer::*;

mod warp_bezier;

mod mesh_generation_scene;
