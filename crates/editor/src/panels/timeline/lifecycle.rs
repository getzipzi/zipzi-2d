//! Interaction lifecycle and reconciliation with the editor's current target.
//! Target changes and stale tokens only reconcile local UI state. Explicit
//! cancellation redraws the owner; dock deactivation defers that work until the
//! owner is released. Keep those notification and cancellation boundaries distinct.
use super::*;

/// The domain operation behind local pointer/number state. This is the common
/// source for capture validity, reconciliation and delayed cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimelineOperation {
    Navigation(editor_core::TimelineBinding),
    Scrub(editor_core::TimelineScrubToken),
    Edit(editor_core::TimelineBinding, TimelineCandidateIdentity),
}
impl TimelineOperation {
    pub(super) fn binding(self) -> editor_core::TimelineBinding {
        match self {
            Self::Navigation(binding) | Self::Edit(binding, _) => binding,
            Self::Scrub(token) => token.binding,
        }
    }
    pub(crate) fn valid(self, editor: &editor_core::Editor) -> bool {
        match self {
            Self::Navigation(binding) => editor.timeline_binding_valid(binding),
            Self::Scrub(token) => editor.timeline_scrub_valid(token),
            Self::Edit(binding, token) => editor.timeline_edit_owned(binding, token),
        }
    }
    fn cancel(self, owner: &mut EditorView) {
        match self {
            Self::Edit(binding, token) if owner.editor.timeline_edit_owned(binding, token) => {
                owner
                    .editor
                    .cancel_timeline_edit_for(binding.document, token);
            }
            Self::Scrub(token) if owner.editor.cancel_timeline_scrub(token) => {
                owner.animation_anchor = None;
            }
            _ => {}
        }
    }
}
pub(crate) struct InteractionCancellation(Option<TimelineOperation>);
impl InteractionCancellation {
    pub(crate) fn apply(self, owner: &mut EditorView) {
        if let Some(operation) = self.0 {
            operation.cancel(owner);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) binding: editor_core::TimelineBinding,
    pub(super) document: DocumentId,
    pub(super) workspace: u64,
    pub(super) model: u64,
    pub(super) clip: String,
    pub(super) mode: AnimationPreviewMode,
}

impl Timeline {
    pub(super) fn target(&self, cx: &App) -> Option<Target> {
        let owner = self.owner.upgrade()?;
        let view = owner.read(cx);
        let animation = view.editor.animation()?;
        let binding = view.editor.timeline_binding(self.id)?;
        (view.editor.editing_context() == Some(editor_core::EditingContext::Animation)
            && !view.modal
            && !view.editor.is_busy()
            && !view.editor.exiting())
        .then(|| Target {
            binding,
            document: view.editor.active_id().unwrap(),
            workspace: view.editor.workspace_revision(),
            model: animation.model_generation,
            clip: animation
                .motion
                .as_ref()
                .map(|motion| motion.clip_id.clone())
                .unwrap_or_default(),
            mode: animation.preview_mode,
        })
    }
    pub(super) fn allowed(&self, window: &mut Window, cx: &mut App) -> bool {
        !window.has_active_dialog(cx) && self.target(cx).is_some()
    }
    pub(super) fn editable(&self, window: &mut Window, cx: &mut App) -> bool {
        self.allowed(window, cx)
            && self.owner.upgrade().is_some_and(|owner| {
                let view = owner.read(cx);
                !view.editor.animation().unwrap().playing
                    && view
                        .editor
                        .timeline_projection()
                        .is_some_and(|p| p.editable)
            })
    }
    pub(super) fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let scale = compact::scale(cx);
        if self.ui_scale != scale {
            self.cancel(window, cx);
            self.ui_scale = scale;
        }
        let placeholder = t!("timeline.search");
        if self.search_placeholder != placeholder.as_ref() {
            self.search_placeholder = placeholder.into_owned();
            self.search.update(cx, |input, cx| {
                input.set_placeholder(self.search_placeholder.clone(), window, cx)
            });
        }
        self.reconcile_target(window, cx);
        self.discard_stale_interactions(window, cx);
        if window.has_active_dialog(cx) && (self.gesture.is_some() || self.number_edit.is_some()) {
            self.cancel(window, cx);
        }
        self.sync_search(window, cx);
        self.sync_playback_speed(window, cx);
    }

    fn reconcile_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.target(cx);
        if target != self.context {
            self.cancel_local(window, cx);
            // A commit changes the model revision, but surviving field entities
            // must retain focus (including a field just focused by a blur).
            // The full target comparison above still invalidates scene geometry.
            let same_clip = self
                .context
                .as_ref()
                .zip(target.as_ref())
                .is_some_and(|(old, new)| {
                    old.document == new.document
                        && old.binding == new.binding
                        && old.workspace == new.workspace
                        && old.clip == new.clip
                        && old.mode == new.mode
                });
            if same_clip {
                let projection = self
                    .owner
                    .upgrade()
                    .and_then(|owner| owner.read(cx).editor.timeline_projection());
                self.fields.retain(|field, _| {
                    projection
                        .as_ref()
                        .is_some_and(|projection| field.exists(projection))
                });
            } else {
                self.fields.clear();
                self.range_key = None;
                self.error = None;
            }
            self.scene.borrow_mut().scene = None;
            if target.as_ref().is_none_or(|target| target.clip.is_empty()) {
                self.tracks.update(cx, |tracks, cx| tracks.clear(cx));
            }
            self.context = target;
        }
    }

    pub(super) fn discard_stale_interactions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let valid = self.operation().is_none_or(|operation| {
            self.owner
                .upgrade()
                .is_some_and(|owner| operation.valid(&owner.read(cx).editor))
        });
        if !valid {
            let editing_number = self.number_edit.is_some();
            self.cancel_local(window, cx);
            if editing_number {
                window.focus(&self.focus, cx);
            }
        }
    }
    pub(super) fn operation(&self) -> Option<TimelineOperation> {
        let binding = self.context.as_ref()?.binding;
        if let Some(input::Gesture::Seek(token)) = self.gesture.as_ref() {
            return Some(TimelineOperation::Scrub(*token));
        }
        if let Some(token) = self
            .gesture
            .as_ref()
            .and_then(input::Gesture::token)
            .or_else(|| self.number_edit.as_ref().map(|edit| edit.token))
        {
            return Some(TimelineOperation::Edit(binding, token));
        }
        self.gesture
            .as_ref()
            .map(|_| TimelineOperation::Navigation(binding))
    }

    fn sync_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            let search = owner
                .read(cx)
                .editor
                .timeline_view()
                .map(|view| view.search.clone())
                .unwrap_or_default();
            if !self.search.read(cx).focus_handle(cx).is_focused(window)
                && self.search.read(cx).value().as_ref() != search
            {
                self.search
                    .update(cx, |input, cx| input.set_value(search, window, cx));
            }
        }
    }

    pub(crate) fn cancel_local(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> InteractionCancellation {
        let cancellation = InteractionCancellation(self.operation());
        if let Some(edit) = self.number_edit.take() {
            if let Some(drag) = edit.drag {
                cx.stop_drag(&drag, window);
            }
            if let Some(control) = self.fields.get(&edit.field) {
                control
                    .number
                    .update(cx, |number, cx| number.reset_drag(cx));
            }
        }
        self.gesture = None;
        self.edge_scroll = None;
        self.error = None;
        self.sync_capture_hint();
        cx.notify();
        cancellation
    }

    pub(crate) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cancellation = self.cancel_local(window, cx);
        let _ = self.owner.update(cx, |owner, cx| {
            cancellation.apply(owner);
            owner.redraw(cx);
        });
        cx.notify();
    }

    pub(crate) fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cancellation = self.cancel_local(window, cx);
        if !matches!(
            cancellation.0,
            Some(TimelineOperation::Scrub(_) | TimelineOperation::Edit(..))
        ) {
            return;
        }
        let owner = self.owner.clone();
        // Dock callbacks may run inside an EditorView update. Resolve the owner
        // after it is released, and never cancel a newer gesture or another tab.
        cx.defer(move |cx| {
            let _ = owner.update(cx, |owner, cx| {
                cancellation.apply(owner);
                owner.redraw(cx);
            });
        });
    }
}
