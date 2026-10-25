//! Optional, immutable UI context. Model queries never depend on a window.
use crate::*;

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct EditorContext {
    pub version: zipzi_sdk::Version,
    pub view_id: String,
    pub selected_ids: Vec<String>,
    pub workspace: String,
    pub editing_context: String,
    pub active_tool: String,
    pub pending_interaction: bool,
}
/// Capture on the UI thread before acquiring the Runtime write borrow.
pub trait EditorContextProvider {
    fn editor_context(&self, session: SessionId) -> Option<EditorContext>;
}
