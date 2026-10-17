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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditingContextToken {
    pub document: DocumentId,
    generation: u64,
}
impl Editor {
    /// Whether ending editing would stop live playback, a pointer operation or
    /// an uncommitted candidate. Local UI may outlive these domain operations.
    pub fn editing_active(&self) -> bool {
        self.viewport_interaction_active()
            || self.gesture_active()
            || self.timeline_edit().is_some()
            || self
                .animation()
                .is_some_and(|animation| animation.playing || animation.is_scrubbing())
    }

    pub(crate) fn advance_editing_context(&mut self) {
        self.context_generation += 1;
        self.editing_epoch += 1;
    }
    pub fn editing_context_token(&self) -> Option<EditingContextToken> {
        self.active_id()
            .filter(|_| !self.is_busy() && !self.exiting())
            .map(|document| EditingContextToken {
                document,
                generation: self.editing_epoch,
            })
    }
    pub fn editing_context_token_valid(&self, token: EditingContextToken) -> bool {
        self.editing_context_token() == Some(token)
    }
    pub fn end_editing(&mut self, document: DocumentId, reason: EditingLifecycle) -> Result<()> {
        if self
            .timeline_scrub_owner
            .is_some_and(|token| token.binding.document == document)
        {
            self.timeline_scrub_owner = None;
        }
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
            // Losing OS focus releases gestures, but keeps dialog drafts in the
            // same document context. Their version guards still apply on return.
            if reason == EditingLifecycle::WindowDeactivated {
                self.context_generation += 1;
            } else {
                self.advance_editing_context();
            }
            self.invalidate_viewport_bindings();
        }
        Ok(())
    }
}
