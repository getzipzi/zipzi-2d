//! Selection-dependent inspector composition.
use super::*;
impl Timeline {
    pub(super) fn render_selection<E: ParentElement>(
        &mut self,
        content: E,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        time: f64,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> E {
        let mut content =
            self.render_selected_key(content, projection, state, disabled, window, cx);
        if state.keys.iter().any(|key| {
            projection
                .key(key)
                .is_some_and(|key| key.outgoing.is_some())
        }) {
            content = self.render_interpolation(content, projection, state, disabled, cx);
        } else if state.events.len() == 1 {
            content = self.render_selected_event(content, projection, state, disabled, window, cx);
        } else {
            content = self
                .render_selected_channel(content, projection, state, time, disabled, window, cx);
        }
        self.render_event_list(content, projection, state, cx)
    }
}
