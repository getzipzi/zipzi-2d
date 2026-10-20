//! Owner notifications are broad; only inspector dependencies invalidate this view.
use super::*;

#[derive(PartialEq, Eq)]
pub(super) struct ObservedState {
    target: Option<PropertyTarget>,
    context: Option<editor_core::EditingContextToken>,
    revision: Option<u64>,
    details: Option<properties::DetailKey>,
    locale: String,
    blocked: bool,
    preview_owned: bool,
    playing: bool,
    sample_time: Option<u32>,
}

impl PropertiesPanel {
    fn observed_state(&self, state: &EditorView) -> ObservedState {
        ObservedState {
            target: properties::target(&state.editor),
            context: state.editor.editing_context_token(),
            revision: state.editor.document().map(|doc| doc.properties_revision()),
            details: properties::Details::key(&state.editor),
            locale: rust_i18n::locale().to_string(),
            blocked: state.modal || state.editor.is_busy() || state.editor.exiting(),
            playing: state.editor.animation().is_some_and(|a| a.playing),
            sample_time: properties::target(&state.editor)
                .and_then(|target| target.sample_time(&state.editor)),
            preview_owned: self
                .preview
                .as_ref()
                .is_none_or(|(_, token)| token.owned(&state.editor)),
        }
    }

    pub(super) fn observe_owner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        let observed = self.observed_state(state);
        let form_changed = self.target.as_ref().is_some_and(|target| {
            state
                .property_forms
                .entries
                .get(target)
                .is_some_and(|form| form != &self.form)
        });
        if self.observed.as_ref() == Some(&observed) && !form_changed {
            return;
        }
        self.sync_form(window, cx);
        self.observed = Some(self.observed_state(owner.read(cx)));
        cx.notify();
    }

    pub(super) fn remember_observed(&mut self, cx: &App) {
        if let Some(owner) = self.owner.upgrade() {
            self.observed = Some(self.observed_state(owner.read(cx)));
        }
    }
}
