//! Shared policy boundary for editor ownership transitions. SDK sessions retain
//! responsibility for candidate commits and history; this layer releases owners.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditingLifecycle {
    ContextChange,
    BlockingUi,
    WindowDeactivated,
    Save,
    Close,
}
impl Editor {
    pub fn end_editing(&mut self, document: DocumentId, reason: EditingLifecycle) -> Result<()> {
        let session = self.entry_mut(document)?.session_mut();
        if let Some(session) = session {
            session.animation.playback.pause();
            match reason {
                EditingLifecycle::BlockingUi | EditingLifecycle::WindowDeactivated => {
                    session.cancel_gesture();
                    session.cancel_timeline_edit();
                }
                EditingLifecycle::Save => session.execute(EditorCommand::FinishEdit)?,
                EditingLifecycle::ContextChange | EditingLifecycle::Close => {
                    session.cancel_timeline_edit();
                    session.execute(EditorCommand::FinishEdit)?;
                }
            }
        } else {
            ensure!(reason != EditingLifecycle::Save, "Document is saving");
        }
        if self
            .viewport_interaction
            .is_some_and(|token| token.binding.document == document)
        {
            self.viewport_interaction = None;
        }
        if self.active_id() == Some(document) {
            self.context_generation += 1;
            self.invalidate_viewport_bindings();
        }
        Ok(())
    }
}
