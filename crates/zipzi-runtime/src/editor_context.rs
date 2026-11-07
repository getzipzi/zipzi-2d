//! Optional, immutable UI context. Model queries never depend on a window.
use crate::*;

pub use zipzi_contract::kernel_wire::EditorContext;
/// Capture on the UI thread before acquiring the Runtime write borrow.
pub trait EditorContextProvider {
    fn editor_context(&self, session: SessionKey) -> Option<EditorContext>;
}
