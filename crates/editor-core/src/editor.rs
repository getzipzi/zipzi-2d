use crate::{Document, SceneSnapshot, ViewState};
use anyhow::{Result, ensure};
use kasane_sdk::SaveReceipt;
use std::path::{Path, PathBuf};

/// UI-independent editing intent. Model changes always go through Document and the SDK.
pub enum EditorCommand {
    Select(String),
    PreviewName(String),
    PreviewOpacity(f32),
    SetVisible(bool),
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
}

/// Editor state and operation coordination. UI adapters cannot mutate the document directly.
pub struct Editor {
    document: Document,
    view: ViewState,
    busy: bool,
    revision: u64,
}

impl Editor {
    pub fn new() -> Result<Self> {
        Ok(Self::from_document(Document::new_blank()?))
    }

    pub fn from_document(document: Document) -> Self {
        Self {
            document,
            view: ViewState::default(),
            busy: false,
            revision: 0,
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn view(&self) -> ViewState {
        self.view
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn can_undo(&self) -> bool {
        !self.busy && (self.document.history_lengths().0 > 0 || self.document.has_pending_edit())
    }

    pub fn can_redo(&self) -> bool {
        !self.busy && self.document.history_lengths().1 > 0 && !self.document.has_pending_edit()
    }

    pub fn execute(&mut self, command: EditorCommand) -> Result<()> {
        ensure!(!self.busy, "Editor is busy");
        let result = match command {
            EditorCommand::Select(id) => self.document.select(&id),
            EditorCommand::PreviewName(name) => self.document.preview_name(&name),
            EditorCommand::PreviewOpacity(value) => self.document.preview_opacity(value),
            EditorCommand::SetVisible(value) => self.document.set_visible(value),
            EditorCommand::FinishEdit => self.document.finish_edit(),
            EditorCommand::CancelEdit => {
                self.document.cancel_edit();
                Ok(())
            }
            EditorCommand::Undo => self.document.undo(),
            EditorCommand::Redo => self.document.redo(),
        };
        // Failed edits may discard a candidate too. Never accept an older preview afterwards.
        self.revision += 1;
        result
    }

    /// Freeze editing while the host awaits a dialog or background operation.
    pub fn begin_operation(&mut self) -> Result<()> {
        ensure!(!self.busy, "Editor is busy");
        self.document.finish_edit()?;
        self.busy = true;
        Ok(())
    }

    pub fn end_operation(&mut self) {
        self.busy = false;
    }

    /// Failed loads preserve the current document and viewport.
    pub fn finish_load(&mut self, result: Result<(Document, Vec<String>)>) -> Result<Vec<String>> {
        ensure!(self.busy, "No editor operation is active");
        self.busy = false;
        let (document, notices) = result?;
        self.document = document;
        self.view = ViewState::default();
        self.revision += 1;
        Ok(notices)
    }

    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        ensure!(!self.busy, "Editor is busy");
        self.document.save(path)
    }

    pub fn close_disposition(&self) -> CloseDisposition {
        if self.busy {
            CloseDisposition::Busy
        } else if self.document.modified() {
            CloseDisposition::NeedsConfirmation
        } else {
            CloseDisposition::Ready
        }
    }

    pub fn zoom_by(&mut self, delta: f32) {
        if delta.is_finite() {
            self.view.zoom = (self.view.zoom + delta).clamp(0.25, 3.);
            self.revision += 1;
        }
    }

    pub fn reset_view(&mut self) {
        self.view = ViewState::default();
        self.revision += 1;
    }

    pub fn preview(&mut self) -> Result<EditorPreview> {
        self.revision += 1;
        Ok(EditorPreview {
            revision: self.revision,
            scene: self.document.snapshot()?,
            view: self.view,
        })
    }

    pub fn accepts_preview(&self, revision: u64) -> bool {
        revision == self.revision
    }
}
