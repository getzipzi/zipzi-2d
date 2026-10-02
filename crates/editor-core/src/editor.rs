use crate::{Document, SceneSnapshot, ViewState};
use anyhow::Result;
use std::path::PathBuf;

/// UI-independent editing intent. Model changes always go through Document and the SDK.
pub enum EditorCommand {
    SelectObjects(Vec<crate::ObjectRef>),
    SetTool(crate::ToolId),
    ToggleDeformers,
    ToggleAncestorDeformers,
    ToggleDeformerGrid,
    ToggleWarpFine,
    AdjustDeformer {
        object: crate::ObjectRef,
        adjustment: crate::DeformerAdjustment,
    },
    SetObjectProperty {
        object: crate::ObjectRef,
        field: String,
        value: serde_json::Value,
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
    SetVisible(bool),
    SetLayerVisible {
        layer_id: String,
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
    Psd {
        source: PathBuf,
        destination: PathBuf,
    },
}

impl LoadRequest {
    pub fn load(self) -> Result<(Document, Vec<String>)> {
        match self {
            Self::Open(path) => Ok((Document::open(&path)?, Vec::new())),
            Self::Model3(path) => Document::import_model3(&path),
            Self::Psd {
                source,
                destination,
            } => Document::import_psd(&source, &destination),
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
