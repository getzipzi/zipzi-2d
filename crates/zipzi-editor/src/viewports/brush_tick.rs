//! Drive continuous brushes only while their viewport owns the pointer capture.
use super::*;

impl ViewportSurface {
    pub(super) fn advance_brush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((token, previous)) = self.brush_tick else {
            return;
        };
        let valid = self.interaction == Some(token)
            && self
                .owner
                .upgrade()
                .is_some_and(|owner| owner.read(cx).editor.brush_inflating_for(token.binding));
        if !valid {
            self.brush_tick = None;
            return;
        }
        let now = cx.background_executor().now();
        self.brush_tick = Some((token, now));
        self.input(
            CanvasInput::BrushTick {
                elapsed: now.saturating_duration_since(previous),
            },
            window,
            cx,
        );
    }

    pub(super) fn schedule_brush_frame(&mut self, window: &Window, cx: &Context<Self>) {
        let token = self.interaction.filter(|token| {
            self.owner
                .upgrade()
                .is_some_and(|owner| owner.read(cx).editor.brush_inflating_for(token.binding))
        });
        let Some(token) = token else {
            self.brush_tick = None;
            return;
        };
        if self.brush_tick.is_none_or(|(current, _)| current != token) {
            self.brush_tick = Some((token, cx.background_executor().now()));
        }
        if self.brush_frame_pending {
            return;
        }
        self.brush_frame_pending = true;
        let view = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = view.update(cx, |this, cx| {
                this.brush_frame_pending = false;
                this.advance_brush(window, cx);
            });
        });
    }
}
