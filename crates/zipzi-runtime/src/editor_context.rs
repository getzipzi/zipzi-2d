//! Optional, immutable UI context. Model queries never depend on a window.
use crate::*;

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct EditorContext {
    /// Exact SDK version captured for this value; not a best-effort latest-version marker.
    pub version: zipzi_sdk::Version,
    /// Identity of the Editor view that supplied this UI context.
    pub view_id: String,
    /// Stable document IDs selected in that Editor view, in its selection order.
    pub selected_ids: Vec<String>,
    /// Editor workspace name captured with the context.
    pub workspace: String,
    /// Active Editor editing context captured with the view.
    pub editing_context: String,
    /// Editor tool name captured with the context.
    pub active_tool: String,
    /// Whether an unfinished UI interaction was present when the context was captured.
    pub pending_interaction: bool,
}
/// Capture on the UI thread before acquiring the Runtime write borrow.
pub trait EditorContextProvider {
    fn editor_context(&self, session: SessionId) -> Option<EditorContext>;
}
