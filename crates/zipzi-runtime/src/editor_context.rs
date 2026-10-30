//! Optional, immutable UI context. Model queries never depend on a window.
use crate::*;

pub use zipzi_contract::runtime::EditorContext;
/// Capture on the UI thread before acquiring the Runtime write borrow.
pub trait EditorContextProvider {
    fn editor_context(&self, session: SessionId) -> Option<EditorContext>;
}
