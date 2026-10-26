use crate::{Document, SceneSnapshot, ViewState};
use anyhow::Result;
use std::path::PathBuf;

/// UI-independent editing intent. Model changes always go through Document and the SDK.
pub enum EditorCommand {
    ReplacePhysics {
        asset: zipzi_core::document::PhysicsAsset,
        expected_revision: u64,
    },
    CreateParameter(crate::NewParameter),
    CreateParameterChecked {
        draft: crate::ParameterDraft,
        expected_version: u64,
    },
    UpdateParameter {
        id: String,
        draft: crate::ParameterDraft,
        expected_version: u64,
    },
    DeleteParameter {
        id: String,
        expected_version: u64,
    },
    CreateParameterGroup {
        draft: crate::ParameterGroupDraft,
        expected_version: u64,
    },
    UpdateParameterGroup {
        id: String,
        draft: crate::ParameterGroupDraft,
        expected_version: u64,
    },
    DeleteParameterGroup {
        id: String,
        expected_version: u64,
    },
    SelectObjects(Vec<crate::ObjectRef>),
    SelectCanvasObjects(Vec<crate::ObjectRef>),
    SetObjectsLocked {
        objects: Vec<crate::ObjectRef>,
        locked: bool,
    },
    SetTool(crate::ToolId),
    GenerateMeshGrid {
        object: crate::ObjectRef,
        options: crate::MeshGridOptions,
        expected_revision: u64,
    },
    ApplyMeshGeneration(std::sync::Arc<crate::MeshGenerationPreview>),
    ToggleDeformers,
    ToggleAncestorDeformers,
    ToggleDeformerGrid,
    ToggleWarpFine,
    ToggleOriginalPoints,
    AdjustDeformer {
        object: crate::ObjectRef,
        adjustment: crate::DeformerAdjustment,
    },
    SetObjectProperty {
        object: crate::ObjectRef,
        field: String,
        value: serde_json::Value,
    },
    SetObjectProperties {
        object: crate::ObjectRef,
        patch: crate::PropertyPatch,
    },
    Select(String),
    SelectObject(crate::ObjectRef),
    FocusParameter(String),
    OpenBinding(crate::ObjectRef),
    Navigate(crate::ObjectRef),
    NavigateBack,
    PreviewKeyform {
        binding: crate::ObjectRef,
        point: usize,
    },
    PreviewName(String),
    PreviewOpacity(f32),
    SetPreviewParameter {
        parameter_id: String,
        value: f32,
    },
    SetPreviewParameters {
        values: Vec<(String, f32)>,
    },
    EditSelectedParameterKeyforms {
        parameter_id: String,
        action: crate::ParameterKeyformAction,
    },
    SetObjectVisible {
        object: crate::ObjectRef,
        visible: bool,
    },
    FinishEdit,
    CancelEdit,
    Undo,
    Redo,
}

/// File paths are supplied by the host; loading can run on its background executor.
pub enum LoadRequest {
    Open(PathBuf),
    Model3(PathBuf),
    Psd(PathBuf),
    Cubism(PathBuf),
}

impl LoadRequest {
    pub fn load(self) -> Result<(Document, Vec<String>)> {
        match self {
            Self::Open(path) => Ok((Document::open(&path)?, Vec::new())),
            Self::Model3(path) => Document::import_model3(&path),
            Self::Psd(source) => Document::import_psd(&source),
            Self::Cubism(source) => Document::import_cubism(&source),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDisposition {
    Busy,
    NeedsConfirmation,
    Ready,
}

/// CPU-only preview request, independent of the host's GPU target and worker.
pub struct EditorPreview {
    pub revision: u64,
    pub scene: SceneSnapshot,
    pub view: ViewState,
    pub canvas: Option<crate::CanvasFrame>,
}
