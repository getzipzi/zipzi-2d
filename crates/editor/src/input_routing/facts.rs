//! Read current hitboxes, focus, capture validity and event flags once per key.
use super::{
    policy::{CaptureFacts, FocusFacts, KeyFacts},
    *,
};
use gpui_kit::component::WindowExt;

pub(super) struct KeySnapshot {
    pub facts: KeyFacts,
    pub capture: Option<CapturedInput>,
}

impl EditorView {
    pub(super) fn resolved_regions(
        &self,
        window: &Window,
        cx: &App,
    ) -> Vec<(InputTarget, InputRegion)> {
        self.input_regions(window, cx)
            .into_iter()
            .filter_map(|region| Some((region.resolve(&self.editor, cx)?, region)))
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn hovered_target(&self, window: &Window, cx: &App) -> Option<LocalTarget> {
        self.hovered_in(&self.resolved_regions(window, cx), window)
            .map(|target| target.local)
    }
    fn hovered_in(
        &self,
        regions: &[(InputTarget, InputRegion)],
        window: &Window,
    ) -> Option<InputTarget> {
        (self.input_router.inside.get() && window.is_visible())
            .then(|| {
                regions
                    .iter()
                    .find(|(_, region)| region.hovered(window))
                    .map(|(target, _)| *target)
            })
            .flatten()
    }
    pub(super) fn collect_key_facts(
        &self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> KeySnapshot {
        let mut snapshot = KeySnapshot {
            facts: KeyFacts {
                focus: FocusFacts::default(),
                prefer_character_input: event.prefer_character_input,
                held: event.is_held,
                escape: event.keystroke.key == "escape",
                unmodified: !event.keystroke.modifiers.modified(),
                capture: None,
                hovered: None,
                focused: None,
            },
            capture: None,
        };
        if event.prefer_character_input {
            return snapshot;
        }
        let regions = self.resolved_regions(window, cx);
        snapshot.facts.focus = FocusFacts {
            blocked: self.modal
                || window.has_active_dialog(cx)
                || window.has_active_sheet(cx)
                || cx.has_active_drag(),
            text_focused: self.has_focused_input(window, cx),
            editor_focused: window.focused(cx).is_none()
                || self.focus.is_focused(window)
                || regions
                    .iter()
                    .any(|(_, region)| region.owns_keyboard(window)),
        };
        snapshot.capture = CapturedInput::first_valid(
            regions
                .iter()
                .filter_map(|(_, region)| region.captured(&self.editor, cx)),
        );
        snapshot.facts.capture = snapshot.capture.as_ref().map(|capture| CaptureFacts {
            target: capture.target,
            cancel_key: capture.cancel_key,
        });
        snapshot.facts.hovered = self.hovered_in(&regions, window);
        snapshot.facts.focused = regions
            .iter()
            .find(|(_, region)| region.owns_keyboard(window))
            .map(|(target, _)| *target);
        snapshot
    }
}
