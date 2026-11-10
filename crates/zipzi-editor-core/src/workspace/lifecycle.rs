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

/// Source of a captured parameter preview. A successful write advances only
/// the preview revision owned by this capture.
pub struct ParameterPreviewInput {
    context: EditingContextToken,
    model_revision: u64,
    preview_revision: u64,
    tool: crate::ToolId,
    selection: Vec<crate::ObjectRef>,
    focus: Option<crate::ObjectRef>,
    binding: Option<crate::ObjectRef>,
}

impl ParameterPreviewInput {
    pub fn valid(&self, editor: &Editor) -> bool {
        editor.editing_context_token_valid(self.context)
            && editor.editing_context() == Some(crate::EditingContext::Modeling)
            && !editor.editing_input_active()
            && !editor.manual_active()
            && editor.active_tool() == self.tool
            && editor.document().is_some_and(|doc| {
                let context = doc.context();
                doc.model_generation() == self.model_revision
                    && doc.preview_revision() == self.preview_revision
                    && context.selections == self.selection
                    && context.parameter_focus == self.focus
                    && context.active_binding == self.binding
            })
    }
}

impl Editor {
    pub fn parameter_preview_input(&self) -> Option<ParameterPreviewInput> {
        if self.editing_context() != Some(crate::EditingContext::Modeling)
            || self.editing_input_active()
            || self.manual_active()
        {
            return None;
        }
        let document = self.document()?;
        let context = document.context();
        Some(ParameterPreviewInput {
            context: self.editing_context_token()?,
            model_revision: document.model_generation(),
            preview_revision: document.preview_revision(),
            tool: self.active_tool(),
            selection: context.selections.clone(),
            focus: context.parameter_focus.clone(),
            binding: context.active_binding.clone(),
        })
    }

    pub fn apply_parameter_preview_input(
        &mut self,
        input: &mut ParameterPreviewInput,
        values: Vec<(String, f32)>,
    ) -> Result<bool> {
        self.sync_runtime()?;
        if !input.valid(self) {
            return Ok(false);
        }
        let session = self
            .entry_mut(input.context.document)?
            .session_mut()
            .unwrap();
        session.execute_parameter_preview_at(values, input.model_revision)?;
        input.preview_revision = session.document().preview_revision();
        Ok(true)
    }

    /// Whether ending editing would stop live playback, a pointer operation or
    /// an uncommitted candidate. Local UI may outlive these domain operations.
    pub fn editing_active(&self) -> bool {
        self.editing_input_active() || self.animation().is_some_and(|animation| animation.playing)
    }
    /// Active input ownership, independent of ordinary transport playback.
    pub fn editing_input_active(&self) -> bool {
        self.viewport_interaction_active()
            || self.document().is_some_and(|doc| doc.has_pending_edit())
            || self.gesture_active()
            || self.timeline_edit().is_some()
            || self
                .animation()
                .is_some_and(|animation| animation.is_scrubbing())
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
        if let Some(token) = self.viewport_interaction.filter(|token| {
            token.binding.document == document
                && token.kind == crate::ViewportInteractionKind::CursorPreview
        }) {
            self.cancel_viewport_interaction(token);
        }
        if self
            .timeline_scrub_owner
            .is_some_and(|token| token.binding.document == document)
        {
            self.timeline_scrub_owner = None;
        }
        let session = self.entry_mut(document)?.session_mut();
        if let Some(session) = session {
            if session.animation.playback.cursor_physics.is_some() {
                session.animation.playback.barrier();
            } else {
                session.animation.playback.pause();
            }
            if reason != EditingLifecycle::Save && session.document.has_inspector_edit() {
                session.document.cancel_edit();
                session.animation.playback.property_preview_changed();
                session.invalidate_scene();
            }
            match reason {
                EditingLifecycle::BlockingUi | EditingLifecycle::WindowDeactivated => {
                    session.cancel_gesture();
                    session.cancel_timeline_edit();
                }
                EditingLifecycle::Save => session.execute(EditorCommand::FinishEdit)?,
                EditingLifecycle::ContextChange | EditingLifecycle::Close => {
                    session.cancel_timeline_edit();
                    session.execute(EditorCommand::FinishEdit)?;
                    session.modeling.tools.exit_manual(&mut session.document);
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
