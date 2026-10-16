//! Interaction lifecycle and reconciliation with the editor's current target.
//! Target changes and stale tokens only reconcile local UI state. Explicit
//! cancellation redraws the owner; dock deactivation defers that work until the
//! owner is released. Keep those notification and cancellation boundaries distinct.
use super::*;

#[derive(Default)]
struct DragOwner(Option<(AnyWindowHandle, editor_core::TimelineBinding, u64)>);
impl Global for DragOwner {}

pub(crate) struct InteractionCancellation {
    binding: Option<editor_core::TimelineBinding>,
    token: Option<TimelineCandidateIdentity>,
    seeking: bool,
}
impl InteractionCancellation {
    pub(crate) fn apply(self, owner: &mut EditorView) {
        let Some(binding) = self.binding else {
            return;
        };
        if let Some(token) = self.token {
            if owner.editor.timeline_edit_owned(binding, token) {
                owner
                    .editor
                    .cancel_timeline_edit_for(binding.document, token);
            }
        }
        if self.seeking && owner.editor.cancel_timeline_scrub_for(binding) {
            owner.animation_anchor = None;
        }
    }
}

impl Timeline {
    pub(super) fn claim_number_drag(
        &self,
        token: TimelineCandidateIdentity,
        window: &Window,
        cx: &mut App,
    ) {
        if let Some(target) = &self.context {
            cx.set_global(DragOwner(Some((
                window.window_handle(),
                target.binding,
                token.edit,
            ))));
        }
    }
    fn release_number_drag(
        &self,
        token: TimelineCandidateIdentity,
        window: &mut Window,
        cx: &mut App,
    ) {
        let owner = self
            .context
            .as_ref()
            .map(|target| (window.window_handle(), target.binding, token.edit));
        if owner.is_some()
            && cx
                .try_global::<DragOwner>()
                .is_some_and(|drag| drag.0 == owner)
        {
            cx.set_global(DragOwner::default());
            cx.stop_active_drag(window);
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
    }

    fn reconcile_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.target(cx);
        if target != self.context {
            if let Some(edit) = self.number_edit.take() {
                self.release_number_drag(edit.token, window, cx);
                if let Some(control) = self.fields.get(&edit.field) {
                    control
                        .number
                        .update(cx, |number, cx| number.reset_drag(cx));
                }
            }
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
            self.gesture = None;
            self.edge_scroll = None;
            self.number_edit = None;
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
                self.slider = None;
                self.range_selection.clear();
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
        let active = self.owner.upgrade().and_then(|owner| {
            owner
                .read(cx)
                .editor
                .timeline_edit()
                .map(|edit| edit.identity.edit)
        });
        if self
            .number_edit
            .as_ref()
            .is_some_and(|edit| active != Some(edit.token.edit))
        {
            let edit = self.number_edit.take().unwrap();
            window.focus(&self.focus, cx);
            self.release_number_drag(edit.token, window, cx);
            self.reset_number_drags(cx);
        }
        if self
            .gesture
            .as_ref()
            .and_then(input::Gesture::token)
            .is_some_and(|token| active != Some(token.edit))
            || (matches!(self.gesture, Some(input::Gesture::Seek))
                && !self.owner.upgrade().is_some_and(|owner| {
                    self.context.as_ref().is_some_and(|target| {
                        owner.read(cx).editor.timeline_scrub_owned(target.binding)
                    })
                }))
        {
            self.gesture = None;
            self.edge_scroll = None;
        }
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
        // Only starting a new interaction supersedes deferred cleanup. Repeated
        // deactivation must still deliver the first operation's cancellation.
        let token = self
            .gesture
            .as_ref()
            .and_then(input::Gesture::token)
            .or_else(|| self.number_edit.as_ref().map(|edit| edit.token));
        let cancellation = InteractionCancellation {
            binding: self.context.as_ref().map(|target| target.binding),
            token,
            seeking: matches!(self.gesture, Some(input::Gesture::Seek)),
        };
        if let Some(token) = token {
            self.release_number_drag(token, window, cx);
        }
        self.gesture = None;
        self.edge_scroll = None;
        self.number_edit = None;
        self.reset_number_drags(cx);
        self.error = None;
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

    fn reset_number_drags(&self, cx: &mut Context<Self>) {
        let numbers: Vec<_> = self
            .fields
            .values()
            .map(|field| field.number.clone())
            .collect();
        for number in numbers {
            number.update(cx, |number, cx| number.reset_drag(cx));
        }
    }

    pub(crate) fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cancellation = self.cancel_local(window, cx);
        if cancellation.token.is_none() && !cancellation.seeking {
            return;
        }
        let owner = self.owner.clone();
        let activity = self.activity.clone();
        let generation = activity.get();
        // Dock callbacks may run inside an EditorView update. Resolve the owner
        // after it is released, and never cancel a newer gesture or another tab.
        cx.defer(move |cx| {
            let _ = owner.update(cx, |owner, cx| {
                if activity.get() != generation {
                    return;
                }
                cancellation.apply(owner);
                owner.redraw(cx);
            });
        });
    }
}
