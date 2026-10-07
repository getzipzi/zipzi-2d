//! UI-independent editor coordination, SDK-backed document editing and CPU-only previews.
mod document;
mod editor;
mod objects;
mod scene;
mod session;
mod thumbnail;
mod tree;
pub use objects::*;
pub use thumbnail::{MESH_THUMBNAIL_SIZE, MeshTextureRegion};
mod workspace;

pub use document::{
    Document, Layer, NewParameter, ParameterCreateError, ParameterGroupOption, ParameterGroupValue,
    ParameterKey, ParameterValue, ParameterVisual,
};
pub use document::{OrganizationDestination, ParameterDestination, ReorderPosition};
pub use editor::{CloseDisposition, EditorCommand, EditorPreview, LoadRequest};
pub use scene::{SceneSnapshot, SceneTexture, ViewState};
pub use tree::{TreeNode, TreeNodeKind};

use session::DocumentSession;
pub use workspace::{
    DocumentId, DocumentSummary, Editor, LoadCompletion, LoadTask, OperationId, SaveCompletion,
    SaveTask,
};

mod commands;
mod recent_files;
mod settings;
pub use commands::*;
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
